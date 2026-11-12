use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::process::Command;
use tracing::{info, warn};

const YT_DLP_DOWNLOAD_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64";

#[derive(Clone)]
pub struct YtDlp {
    binary: PathBuf,
    videos_dir: PathBuf,
    cache_dir: PathBuf,
}

impl YtDlp {
    pub fn new(state_dir: &Path, runtime_dir: &Path) -> Self {
        Self {
            binary: state_dir.join("yt-dlp"),
            videos_dir: runtime_dir.join("videos"),
            cache_dir: runtime_dir.join("yt-dlp-cache"),
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

    pub async fn download(&self, url: &str) -> Result<PathBuf> {
        let output = Command::new(&self.binary)
            .args([
                "--no-playlist",
                "--no-progress",
                "--no-warnings",
                "--no-simulate",
            ])
            .args(["--format", "bv*[height<=240]/b[height<=240]/wv*/w"])
            .arg("--cache-dir")
            .arg(&self.cache_dir)
            .arg("--output")
            .arg(self.videos_dir.join("%(id)s.%(ext)s"))
            .args(["--print", "after_move:filepath", "--", url])
            .output()
            .await
            .context("running yt-dlp")?;
        if !output.status.success() {
            bail!(
                "yt-dlp failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let path = stdout
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .context("yt-dlp did not report a file")?;
        Ok(PathBuf::from(path))
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
