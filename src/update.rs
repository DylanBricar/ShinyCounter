//! Background GitHub release checker + downloader. One worker thread on
//! launch (and on user request) hits the public Releases API, then optionally
//! streams the matching binary asset into the OS Downloads folder.

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const REPO_OWNER: &str = "DylanBricar";
const REPO_NAME: &str = "ShinyCounter";
const USER_AGENT: &str = concat!("ShinyCounter/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct UpdateAsset {
    pub url: String,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub release_url: String,
    pub release_name: String,
    pub published_at: String,
    pub current_version: String,
    pub asset: Option<UpdateAsset>,
}

#[derive(Debug, Clone, Default)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate {
        current: String,
    },
    Available(UpdateInfo),
    Downloading {
        info: UpdateInfo,
        percent: u8,
    },
    Downloaded {
        info: UpdateInfo,
        path: PathBuf,
    },
    Error(String),
}

#[derive(Default, Clone)]
pub struct UpdateChannel {
    inner: Arc<Mutex<UpdateStatus>>,
}

impl UpdateChannel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self) -> UpdateStatus {
        self.inner.lock().clone()
    }

    pub fn set(&self, status: UpdateStatus) {
        *self.inner.lock() = status;
    }
}

/// Spawn a thread that asks GitHub for the latest release once.
pub fn spawn_check(channel: UpdateChannel) {
    channel.set(UpdateStatus::Checking);
    let _ = thread::Builder::new()
        .name("shiny-counter-update".into())
        .spawn(move || match fetch_latest() {
            Ok(release) => channel.set(decide(release)),
            Err(e) => channel.set(UpdateStatus::Error(e.to_string())),
        });
}

/// Spawn a thread that streams the platform-matching asset to the user's
/// Downloads folder, reporting percentage progress as it goes.
pub fn spawn_download(channel: UpdateChannel, info: UpdateInfo) {
    channel.set(UpdateStatus::Downloading {
        info: info.clone(),
        percent: 0,
    });
    let _ = thread::Builder::new()
        .name("shiny-counter-download".into())
        .spawn({
            let info = info.clone();
            let channel = channel.clone();
            move || match download_asset(&info, |pct| {
                channel.set(UpdateStatus::Downloading {
                    info: info.clone(),
                    percent: pct,
                });
            }) {
                Ok(path) => channel.set(UpdateStatus::Downloaded { info, path }),
                Err(e) => channel.set(UpdateStatus::Error(e.to_string())),
            }
        });
}

fn decide(latest: ReleaseDto) -> UpdateStatus {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let cur_sv = semver::Version::parse(&current).ok();
    let tag = latest.tag_name.trim_start_matches('v').to_string();
    let new_sv = semver::Version::parse(&tag).ok();
    match (cur_sv, new_sv) {
        (Some(c), Some(n)) if n > c => {
            let asset = pick_platform_asset(&latest.assets);
            UpdateStatus::Available(UpdateInfo {
                latest_version: tag,
                release_url: latest.html_url,
                release_name: latest
                    .name
                    .unwrap_or_else(|| format!("v{}", latest.tag_name)),
                published_at: latest.published_at.unwrap_or_default(),
                current_version: current,
                asset,
            })
        }
        (_, _) => UpdateStatus::UpToDate { current },
    }
}

/// The release suffix produced by `.github/workflows/release.yml` for the
/// platform we are currently running on. `None` means no auto-download will
/// be available (rare hosts, fallback to opening the release page).
fn platform_suffix() -> Option<&'static str> {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("windows-x86_64.exe")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("macos-aarch64.dmg")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("macos-x86_64.dmg")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64.tar.gz")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("linux-aarch64.tar.gz")
    } else {
        None
    }
}

fn pick_platform_asset(assets: &[AssetDto]) -> Option<UpdateAsset> {
    let suffix = platform_suffix()?;
    assets
        .iter()
        .find(|a| a.name.ends_with(suffix))
        .map(|a| UpdateAsset {
            url: a.browser_download_url.clone(),
            name: a.name.clone(),
            size: a.size,
        })
}

#[derive(Debug, Deserialize)]
struct ReleaseDto {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<AssetDto>,
}

#[derive(Debug, Deserialize)]
struct AssetDto {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

fn fetch_latest() -> Result<ReleaseDto> {
    let url = format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest");
    let response = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .user_agent(USER_AGENT)
        .build()
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("GET {url}"))?;
    let release: ReleaseDto = response
        .into_json()
        .context("parsing /releases/latest JSON")?;
    if release.draft || release.prerelease {
        return Err(anyhow!("only stable releases are considered for update"));
    }
    Ok(release)
}

fn download_asset(info: &UpdateInfo, on_progress: impl Fn(u8)) -> Result<PathBuf> {
    let asset = info
        .asset
        .as_ref()
        .ok_or_else(|| anyhow!("no platform-matching asset in this release"))?;
    let dir = dirs::download_dir()
        .or_else(dirs::cache_dir)
        .unwrap_or_else(std::env::temp_dir);
    let target_path = dir.join(&asset.name);
    // Write to a `.partial` sidecar so a stale or locked target file can't
    // wedge the download. We rename atomically on success.
    let tmp_path = dir.join(format!("{}.partial", &asset.name));
    let _ = std::fs::remove_file(&tmp_path);

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .user_agent(USER_AGENT)
        .build();
    let response = agent
        .get(&asset.url)
        .set("Accept", "application/octet-stream")
        .call()
        .with_context(|| format!("GET {}", asset.url))?;

    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(&tmp_path)
        .with_context(|| format!("creating {}", tmp_path.display()))?;
    let mut buf = vec![0u8; 64 * 1024];
    let total = asset.size.max(1);
    let mut downloaded: u64 = 0;
    let mut last_pct: u8 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        let pct = ((downloaded.saturating_mul(100)) / total).min(100) as u8;
        if pct != last_pct {
            on_progress(pct);
            last_pct = pct;
        }
    }
    file.flush()?;
    drop(file);
    on_progress(100);
    // Replace any existing copy. If the destination is locked (e.g. the user
    // is currently running it), keep the `.partial` file with a numeric
    // suffix so the download isn't lost.
    let _ = std::fs::remove_file(&target_path);
    if let Err(_e) = std::fs::rename(&tmp_path, &target_path) {
        // Fallback: timestamped name in the same directory.
        let stem = target_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("shiny-counter-update");
        let ext = target_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let alt = if ext.is_empty() {
            target_path.with_file_name(format!("{stem}-{stamp}"))
        } else {
            target_path.with_file_name(format!("{stem}-{stamp}.{ext}"))
        };
        std::fs::rename(&tmp_path, &alt)
            .with_context(|| format!("renaming partial to {}", alt.display()))?;
        return Ok(alt);
    }
    Ok(target_path)
}

/// Open a downloaded file using the OS default handler.
pub fn open_path(path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

/// Open the release page in the system's default browser.
pub fn open_release_page(info: &UpdateInfo) {
    let _ = webbrowser::open(&info.release_url);
}

/// Snooze duration applied when the user clicks "Later".
pub const SNOOZE_DURATION_SECS: i64 = 60 * 60 * 24 * 7; // 7 days

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> ReleaseDto {
        ReleaseDto {
            tag_name: tag.to_string(),
            name: Some(tag.to_string()),
            html_url: "https://example".into(),
            published_at: None,
            draft: false,
            prerelease: false,
            assets: vec![],
        }
    }

    #[test]
    fn decide_returns_available_when_remote_is_newer() {
        let cur = env!("CARGO_PKG_VERSION");
        let mut sv = semver::Version::parse(cur).unwrap();
        sv.patch += 1;
        match decide(release(&format!("v{sv}"))) {
            UpdateStatus::Available(info) => assert_eq!(info.latest_version, sv.to_string()),
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[test]
    fn decide_returns_uptodate_when_remote_is_same_or_older() {
        let cur = env!("CARGO_PKG_VERSION");
        match decide(release(&format!("v{cur}"))) {
            UpdateStatus::UpToDate { current } => assert_eq!(current, cur),
            other => panic!("expected UpToDate, got {other:?}"),
        }
    }

    #[test]
    fn pick_asset_matches_platform_suffix() {
        let assets = vec![
            AssetDto {
                name: "ShinyCounter-1.2.3-linux-x86_64.tar.gz".into(),
                browser_download_url: "https://e/linux".into(),
                size: 10,
            },
            AssetDto {
                name: "ShinyCounter-1.2.3-windows-x86_64.exe".into(),
                browser_download_url: "https://e/win".into(),
                size: 20,
            },
            AssetDto {
                name: "ShinyCounter-1.2.3-macos-aarch64.dmg".into(),
                browser_download_url: "https://e/mac".into(),
                size: 30,
            },
        ];
        let picked = pick_platform_asset(&assets);
        if let Some(s) = platform_suffix() {
            let p = picked.expect("expected an asset for this host");
            assert!(p.name.ends_with(s), "got {}", p.name);
        }
    }
}
