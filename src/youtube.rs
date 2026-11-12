use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use regex::Regex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{info, warn};

const YT_DLP_DOWNLOAD_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64";
const KEPT_VIDEOS: usize = 2;
const PROGRESS_MARKER: &str = "led-lantern-progress:";
const FILE_MARKER: &str = "led-lantern-file:";

static EXTRACTOR_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[[^\]]+\] [^:]+: ").expect("valid regex"));

#[derive(Clone)]
pub struct YtDlp {
    binary: PathBuf,
    videos_dir: PathBuf,
    cache_dir: PathBuf,
    downloaded: Arc<Mutex<RecentDownloads>>,
}

#[derive(Default)]
struct RecentDownloads {
    oldest_first: Vec<(String, PathBuf)>,
}

#[derive(Debug, PartialEq)]
enum OutputLine {
    Progress(u8),
    File(PathBuf),
    Error(String),
    Other,
}

impl YtDlp {
    pub fn new(state_dir: &Path, video_dir: &Path) -> Self {
        Self {
            binary: state_dir.join("yt-dlp"),
            videos_dir: video_dir.join("videos"),
            cache_dir: video_dir.join("yt-dlp-cache"),
            downloaded: Arc::new(Mutex::new(RecentDownloads::default())),
        }
    }

    pub async fn keep_updated(self, http: reqwest::Client) {
        loop {
            let result = match tokio::fs::try_exists(&self.binary).await {
                Ok(true) => self.update().await,
                Ok(false) => self.install(&http).await,
                Err(error) => Err(error.into()),
            };
            if let Err(error) = result {
                warn!("Updating yt-dlp failed: {error:#}");
            }
            tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;
        }
    }

    pub async fn clear_videos(&self) -> Result<()> {
        if tokio::fs::try_exists(&self.videos_dir).await? {
            tokio::fs::remove_dir_all(&self.videos_dir)
                .await
                .context("removing old videos")?;
        }
        tokio::fs::create_dir_all(&self.videos_dir)
            .await
            .context("creating the videos folder")
    }

    pub fn cached(&self, url: &str) -> Option<PathBuf> {
        self.downloaded.lock().ok()?.path_for(url)
    }

    pub async fn download(&self, url: &str, mut on_progress: impl FnMut(u8)) -> Result<PathBuf> {
        let mut child = Command::new(&self.binary)
            .args(["--no-playlist", "--no-warnings", "--no-simulate"])
            .args(["--newline", "--progress", "--progress-template"])
            .arg(format!(
                "download:{PROGRESS_MARKER}%(progress.downloaded_bytes)s/%(progress.total_bytes)s/%(progress.total_bytes_estimate)s"
            ))
            .args(["--format", "bv*[height<=240]/b[height<=240]/wv*/w"])
            .args(["--match-filter", "duration < 3600"])
            .arg("--cache-dir")
            .arg(&self.cache_dir)
            .arg("--output")
            .arg(self.videos_dir.join("%(id)s.%(ext)s"))
            .arg("--print")
            .arg(format!("after_move:{FILE_MARKER}%(filepath)s"))
            .args(["--", url])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("running yt-dlp")?;
        let mut stdout =
            BufReader::new(child.stdout.take().context("yt-dlp has no stdout")?).lines();
        let mut stderr =
            BufReader::new(child.stderr.take().context("yt-dlp has no stderr")?).lines();

        let mut file = None;
        let mut error = None;
        let mut last_percent = None;
        let mut is_stdout_open = true;
        let mut is_stderr_open = true;
        while is_stdout_open || is_stderr_open {
            let (is_from_stdout, line) = tokio::select! {
                line = stdout.next_line(), if is_stdout_open => (true, line.context("reading yt-dlp output")?),
                line = stderr.next_line(), if is_stderr_open => (false, line.context("reading yt-dlp errors")?),
            };
            let Some(line) = line else {
                if is_from_stdout {
                    is_stdout_open = false;
                } else {
                    is_stderr_open = false;
                }
                continue;
            };
            match parse_output_line(&line) {
                OutputLine::Progress(percent) if last_percent != Some(percent) => {
                    last_percent = Some(percent);
                    on_progress(percent);
                }
                OutputLine::File(path) => file = Some(path),
                OutputLine::Error(message) => error = Some(message),
                OutputLine::Progress(_) | OutputLine::Other => {}
            }
        }

        let status = child.wait().await.context("waiting for yt-dlp")?;
        if !status.success() {
            bail!(
                "{}",
                error
                    .as_deref()
                    .unwrap_or("yt-dlp failed without an error message")
            );
        }
        let path = file.context("the video is over an hour long, or is a live stream")?;
        let evicted = match self.downloaded.lock() {
            Ok(mut downloaded) => downloaded.record(url, path.clone()),
            Err(_) => Vec::new(),
        };
        for old_path in evicted {
            if let Err(error) = tokio::fs::remove_file(&old_path).await {
                warn!("Removing {} failed: {error}", old_path.display());
            }
        }
        Ok(path)
    }

    async fn update(&self) -> Result<()> {
        let output = Command::new(&self.binary)
            .arg("--update")
            .output()
            .await
            .context("running yt-dlp --update")?;
        if !output.status.success() {
            bail!(
                "yt-dlp --update failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        info!(
            "{}",
            stdout
                .lines()
                .last()
                .unwrap_or("yt-dlp checked for updates")
        );
        Ok(())
    }

    async fn install(&self, http: &reqwest::Client) -> Result<()> {
        info!("Downloading yt-dlp to {}", self.binary.display());
        let bytes = http
            .get(YT_DLP_DOWNLOAD_URL)
            .send()
            .await
            .context("downloading yt-dlp")?
            .error_for_status()
            .context("downloading yt-dlp")?
            .bytes()
            .await
            .context("downloading yt-dlp")?;
        let partial = self.binary.with_extension("partial");
        tokio::fs::write(&partial, &bytes)
            .await
            .context("saving yt-dlp")?;
        tokio::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))
            .await
            .context("making yt-dlp executable")?;
        tokio::fs::rename(&partial, &self.binary)
            .await
            .context("installing yt-dlp")
    }
}

impl RecentDownloads {
    fn path_for(&self, url: &str) -> Option<PathBuf> {
        self.oldest_first
            .iter()
            .find(|(known_url, _)| known_url == url)
            .map(|(_, path)| path.clone())
    }

    fn record(&mut self, url: &str, path: PathBuf) -> Vec<PathBuf> {
        self.oldest_first.retain(|(known_url, _)| known_url != url);
        self.oldest_first.push((url.to_string(), path));
        let excess = self.oldest_first.len().saturating_sub(KEPT_VIDEOS);
        let evicted: Vec<PathBuf> = self
            .oldest_first
            .drain(..excess)
            .map(|(_, path)| path)
            .collect();
        evicted
            .into_iter()
            .filter(|path| !self.oldest_first.iter().any(|(_, kept)| kept == path))
            .collect()
    }
}

fn parse_output_line(line: &str) -> OutputLine {
    let line = line.trim();
    if let Some(progress) = line.strip_prefix(PROGRESS_MARKER) {
        return progress_percent(progress).map_or(OutputLine::Other, OutputLine::Progress);
    }
    if let Some(path) = line.strip_prefix(FILE_MARKER) {
        return OutputLine::File(PathBuf::from(path));
    }
    if let Some(message) = line.strip_prefix("ERROR: ") {
        return OutputLine::Error(EXTRACTOR_PREFIX.replace(message, "").into_owned());
    }
    OutputLine::Other
}

fn progress_percent(progress: &str) -> Option<u8> {
    let mut fields = progress.split('/').map(|field| field.parse::<f64>().ok());
    let downloaded = fields.next()??;
    let total = fields.flatten().find(|total| *total > 0.0)?;
    Some((downloaded / total * 100.0).clamp(0.0, 100.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_progress_from_total_or_estimate() {
        assert_eq!(
            parse_output_line("led-lantern-progress:420/1000/NA"),
            OutputLine::Progress(42)
        );
        assert_eq!(
            parse_output_line("led-lantern-progress:500/NA/2000.5"),
            OutputLine::Progress(24)
        );
        assert_eq!(
            parse_output_line("led-lantern-progress:500/NA/NA"),
            OutputLine::Other
        );
    }

    #[test]
    fn keeps_only_the_latest_videos() {
        let mut downloads = RecentDownloads::default();
        assert!(downloads.record("a", PathBuf::from("/v/a.webm")).is_empty());
        assert!(downloads.record("b", PathBuf::from("/v/b.webm")).is_empty());
        assert_eq!(
            downloads.record("c", PathBuf::from("/v/c.webm")),
            [PathBuf::from("/v/a.webm")]
        );
        assert_eq!(downloads.path_for("a"), None);
        assert_eq!(downloads.path_for("c"), Some(PathBuf::from("/v/c.webm")));
        assert_eq!(downloads.oldest_first.len(), KEPT_VIDEOS);
    }

    #[test]
    fn keeps_a_file_another_link_still_uses() {
        let mut downloads = RecentDownloads::default();
        downloads.record("https://youtu.be/x", PathBuf::from("/v/x.webm"));
        downloads.record("b", PathBuf::from("/v/b.webm"));
        let evicted = downloads.record(
            "https://www.youtube.com/watch?v=x",
            PathBuf::from("/v/x.webm"),
        );
        assert!(evicted.is_empty());
    }

    #[test]
    fn reads_file_and_error_lines() {
        assert_eq!(
            parse_output_line("led-lantern-file:/run/led-lantern/videos/abc.webm"),
            OutputLine::File(PathBuf::from("/run/led-lantern/videos/abc.webm"))
        );
        assert_eq!(
            parse_output_line("ERROR: [youtube] abc123: Video unavailable"),
            OutputLine::Error("Video unavailable".into())
        );
        assert_eq!(
            parse_output_line("[youtube] Extracting URL"),
            OutputLine::Other
        );
    }
}
