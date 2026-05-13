//! Background GitHub release checker. Hits the public Releases API once at
//! launch (and on user request), parses the latest tag and compares to the
//! current version. No telemetry, no auth, single sync request from a worker
//! thread.

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use serde::Deserialize;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const REPO_OWNER: &str = "DylanBricar";
const REPO_NAME: &str = "ShinyCounter";
const USER_AGENT: &str = concat!("ShinyCounter/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub release_url: String,
    pub release_name: String,
    pub published_at: String,
    pub current_version: String,
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
            Ok(info) => channel.set(decide(info)),
            Err(e) => channel.set(UpdateStatus::Error(e.to_string())),
        });
}

fn decide(latest: ReleaseDto) -> UpdateStatus {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let cur_sv = semver::Version::parse(&current).ok();
    let tag = latest.tag_name.trim_start_matches('v').to_string();
    let new_sv = semver::Version::parse(&tag).ok();
    match (cur_sv, new_sv) {
        (Some(c), Some(n)) if n > c => UpdateStatus::Available(UpdateInfo {
            latest_version: tag,
            release_url: latest.html_url,
            release_name: latest
                .name
                .unwrap_or_else(|| format!("v{}", latest.tag_name)),
            published_at: latest.published_at.unwrap_or_default(),
            current_version: current,
        }),
        (_, _) => UpdateStatus::UpToDate { current },
    }
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

/// Open the release page in the system's default browser.
pub fn open_release_page(info: &UpdateInfo) {
    let _ = webbrowser::open(&info.release_url);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_returns_available_when_remote_is_newer() {
        let cur = env!("CARGO_PKG_VERSION");
        let mut sv = semver::Version::parse(cur).unwrap();
        sv.patch += 1;
        let release = ReleaseDto {
            tag_name: format!("v{sv}"),
            name: Some(format!("v{sv}")),
            html_url: "https://example".into(),
            published_at: None,
            draft: false,
            prerelease: false,
        };
        match decide(release) {
            UpdateStatus::Available(info) => assert_eq!(info.latest_version, sv.to_string()),
            other => panic!("expected Available, got {other:?}"),
        }
    }

    #[test]
    fn decide_returns_uptodate_when_remote_is_same_or_older() {
        let cur = env!("CARGO_PKG_VERSION");
        let release = ReleaseDto {
            tag_name: format!("v{cur}"),
            name: None,
            html_url: "https://example".into(),
            published_at: None,
            draft: false,
            prerelease: false,
        };
        match decide(release) {
            UpdateStatus::UpToDate { current } => assert_eq!(current, cur),
            other => panic!("expected UpToDate, got {other:?}"),
        }
    }
}
