use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use tracing::{info, warn};

use super::frame::{Frame, Rgb};
use super::tiling::{TileSize, fit_tile, tile_across_panel};
use crate::entry::VideoSource;

pub struct Video {
    frames: Receiver<Frame>,
    latest: Frame,
    is_stopping: Arc<AtomicBool>,
    process: Arc<Mutex<Option<Child>>>,
}

struct Worker {
    source: VideoSource,
    frames: SyncSender<Frame>,
    is_stopping: Arc<AtomicBool>,
    process: Arc<Mutex<Option<Child>>>,
}

impl Video {
    pub fn start(source: VideoSource) -> Self {
        let (sender, frames) = mpsc::sync_channel(2);
        let is_stopping = Arc::new(AtomicBool::new(false));
        let process = Arc::new(Mutex::new(None));
        let worker = Worker {
            source,
            frames: sender,
            is_stopping: Arc::clone(&is_stopping),
            process: Arc::clone(&process),
        };
        thread::spawn(move || worker.run());
        Self {
            frames,
            latest: Frame::filled(Rgb::BLACK),
            is_stopping,
            process,
        }
    }

    pub fn latest_frame(&mut self) -> &Frame {
        while let Ok(frame) = self.frames.try_recv() {
            self.latest = frame;
        }
        &self.latest
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        self.is_stopping.store(true, Ordering::SeqCst);
        if let Ok(mut process) = self.process.lock()
            && let Some(child) = process.as_mut()
        {
            let _ = child.kill();
        }
    }
}

impl Worker {
    fn run(self) {
        while !self.is_stopping.load(Ordering::SeqCst) {
            match self.play() {
                Ok(()) => info!(input = %self.source.input, "video ended"),
                Err(error) => warn!(input = %self.source.input, "video failed: {error:#}"),
            }
            if self.is_stopping.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_secs(5));
        }
    }

    fn play(&self) -> Result<()> {
        let (source_width, source_height) = probe(&self.source)?;
        let tile = fit_tile(source_width, source_height);
        info!(input = %self.source.input, source_width, source_height, ?tile, "playing video");

        let mut child = Command::new("ffmpeg")
            .args(ffmpeg_arguments(&self.source, tile))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("starting ffmpeg")?;
        let mut output = child.stdout.take().context("ffmpeg has no stdout")?;
        self.process
            .lock()
            .map_err(|_| anyhow!("video process lock poisoned"))?
            .replace(child);
        if self.is_stopping.load(Ordering::SeqCst) {
            self.kill_process();
        }

        let mut buffer = vec![0; tile.byte_count()];
        while output.read_exact(&mut buffer).is_ok() {
            match self.frames.try_send(tile_across_panel(&buffer, tile)) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => break,
            }
        }
        self.kill_process();
        Ok(())
    }

    fn kill_process(&self) {
        let Ok(mut process) = self.process.lock() else {
            return;
        };
        if let Some(mut child) = process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn probe(source: &VideoSource) -> Result<(usize, usize)> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-show_entries", "stream=width,height", "-of", "csv=p=0"])
        .args(transport_arguments(source))
        .arg(&source.input)
        .output()
        .context("running ffprobe")?;
    if !output.status.success() {
        bail!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse_dimensions(&String::from_utf8_lossy(&output.stdout))
}

fn parse_dimensions(probe_output: &str) -> Result<(usize, usize)> {
    let line = probe_output
        .lines()
        .next()
        .context("ffprobe found no video stream")?;
    let mut fields = line
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty());
    let (Some(width), Some(height)) = (fields.next(), fields.next()) else {
        bail!("unexpected ffprobe output: {line}");
    };
    let width: usize = width.parse().context("parsing video width")?;
    let height: usize = height.parse().context("parsing video height")?;
    if width == 0 || height == 0 {
        bail!("video has no pixels: {line}");
    }
    Ok((width, height))
}

fn transport_arguments(source: &VideoSource) -> &'static [&'static str] {
    if source.is_live {
        &["-rtsp_transport", "tcp"]
    } else {
        &[]
    }
}

fn ffmpeg_arguments(source: &VideoSource, tile: TileSize) -> Vec<String> {
    let looping: &[&str] = if source.is_live {
        &[]
    } else {
        &["-re", "-stream_loop", "-1"]
    };
    ["-nostdin", "-loglevel", "error"]
        .iter()
        .chain(transport_arguments(source))
        .chain(looping)
        .map(|argument| argument.to_string())
        .chain([
            "-i".to_string(),
            source.input.clone(),
            "-an".to_string(),
            "-vf".to_string(),
            format!("fps=30,scale={}:{}:flags=area", tile.width, tile.height),
            "-pix_fmt".to_string(),
            "rgb24".to_string(),
            "-f".to_string(),
            "rawvideo".to_string(),
            "pipe:1".to_string(),
        ])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_probe_output() {
        assert_eq!(parse_dimensions("1920,1080\n").unwrap(), (1920, 1080));
        assert_eq!(parse_dimensions("640,360,\n").unwrap(), (640, 360));
        assert!(parse_dimensions("").is_err());
    }

    #[test]
    fn file_input_loops_in_real_time() {
        let source = VideoSource {
            input: "/videos/clip.webm".into(),
            is_live: false,
        };
        let tile = TileSize {
            width: 57,
            height: 32,
        };
        let arguments = ffmpeg_arguments(&source, tile).join(" ");
        assert!(arguments.contains("-re -stream_loop -1 -i /videos/clip.webm"));
        assert!(arguments.contains("scale=57:32"));
    }

    #[test]
    fn live_input_uses_tcp_and_does_not_loop() {
        let source = VideoSource {
            input: "rtsp://127.0.0.1:8554/stream".into(),
            is_live: true,
        };
        let tile = TileSize {
            width: 57,
            height: 32,
        };
        let arguments = ffmpeg_arguments(&source, tile).join(" ");
        assert!(arguments.contains("-rtsp_transport tcp -i rtsp://"));
        assert!(!arguments.contains("-stream_loop"));
    }
}
