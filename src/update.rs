//! Background GitHub release checker + downloader. One worker thread on
//! launch (and on user request) hits the public Releases API, then optionally
//! streams the matching binary asset into the OS Downloads folder.

use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const REPO_OWNER: &str = "DylanBricar";
const REPO_NAME: &str = "ShinyCounter";
const USER_AGENT: &str = concat!("ShinyCounter/", env!("CARGO_PKG_VERSION"));
const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;
const RELEASE_DOWNLOAD_PREFIX: &str =
    "https://github.com/DylanBricar/ShinyCounter/releases/download/";

#[derive(Debug, Clone)]
pub struct UpdateAsset {
    pub url: String,
    pub name: String,
    pub size: u64,
    pub digest: String,
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

/// Spawn a thread that asks GitHub for the latest release once. A second
/// call while a check is already in flight is dropped silently to keep us
/// well under GitHub's anonymous rate limit (60 req/hour/IP).
pub fn spawn_check(channel: UpdateChannel) {
    if matches!(
        channel.status(),
        UpdateStatus::Checking | UpdateStatus::Downloading { .. }
    ) {
        return;
    }
    channel.set(UpdateStatus::Checking);
    let worker_channel = channel.clone();
    if let Err(e) = thread::Builder::new()
        .name("shiny-counter-update".into())
        .spawn(move || match fetch_latest() {
            Ok(release) => worker_channel.set(decide(release)),
            Err(e) => worker_channel.set(UpdateStatus::Error(e.to_string())),
        })
    {
        channel.set(UpdateStatus::Error(format!(
            "could not start update check: {e}"
        )));
    }
}

/// Spawn a thread that streams the platform-matching asset to the user's
/// Downloads folder, reporting percentage progress as it goes.
///
/// A second concurrent call while another download is already in flight is
/// silently ignored - the existing worker keeps going.
pub fn spawn_download(channel: UpdateChannel, info: UpdateInfo) {
    if matches!(
        channel.status(),
        UpdateStatus::Downloading { .. } | UpdateStatus::Downloaded { .. }
    ) {
        return;
    }
    channel.set(UpdateStatus::Downloading {
        info: info.clone(),
        percent: 0,
    });
    let failure_channel = channel.clone();
    if let Err(e) = thread::Builder::new()
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
        })
    {
        failure_channel.set(UpdateStatus::Error(format!(
            "could not start download: {e}"
        )));
    }
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
    assets.iter().find_map(|asset| {
        let digest = asset.digest.as_deref()?;
        if !asset.name.ends_with(suffix)
            || asset.size == 0
            || asset.size > MAX_ASSET_BYTES
            || !is_trusted_asset_url(&asset.browser_download_url)
            || parse_sha256_digest(digest).is_err()
        {
            return None;
        }
        Some(UpdateAsset {
            url: asset.browser_download_url.clone(),
            name: asset.name.clone(),
            size: asset.size,
            digest: digest.to_owned(),
        })
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
    #[serde(default)]
    digest: Option<String>,
}

fn fetch_latest() -> Result<ReleaseDto> {
    let url = format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .user_agent(USER_AGENT)
        .build()
        .into();
    let mut response = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("GET {url}"))?;
    let release: ReleaseDto = response
        .body_mut()
        .read_json()
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
    validate_asset(asset)?;
    let dir = dirs::download_dir()
        .or_else(dirs::cache_dir)
        .unwrap_or_else(std::env::temp_dir);
    let file_name = safe_asset_file_name(&asset.name);
    let target_path = dir.join(&file_name);
    // Write to a `.partial` sidecar so a stale or locked target file can't
    // wedge the download. We rename atomically on success.
    let tmp_path = dir.join(format!("{file_name}.partial"));
    let _ = std::fs::remove_file(&tmp_path);

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(180)))
        .user_agent(USER_AGENT)
        .build()
        .into();
    let mut response = agent
        .get(&asset.url)
        .header("Accept", "application/octet-stream")
        .call()
        .with_context(|| format!("GET {}", asset.url))?;

    let mut reader = response.body_mut().as_reader();
    write_verified_download(&mut reader, &tmp_path, asset, &on_progress)?;
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

fn write_verified_download<R: Read>(
    reader: &mut R,
    path: &Path,
    asset: &UpdateAsset,
    on_progress: &impl Fn(u8),
) -> Result<()> {
    let result = (|| {
        let mut file =
            std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
        copy_verified(reader, &mut file, asset, on_progress)?;
        file.sync_all()
            .with_context(|| format!("syncing {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

fn copy_verified<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    asset: &UpdateAsset,
    on_progress: &impl Fn(u8),
) -> Result<()> {
    let expected = parse_sha256_digest(&asset.digest)?;
    let mut buf = vec![0_u8; 64 * 1024];
    let mut downloaded = 0_u64;
    let mut last_pct = 0_u8;
    let mut hasher = Sha256::new();
    loop {
        let bytes_read = reader.read(&mut buf)?;
        if bytes_read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(bytes_read as u64)
            .ok_or_else(|| anyhow!("download size overflow"))?;
        if downloaded > asset.size {
            bail!("download exceeded declared size of {} bytes", asset.size);
        }
        writer.write_all(&buf[..bytes_read])?;
        hasher.update(&buf[..bytes_read]);
        let progress = ((downloaded.saturating_mul(100)) / asset.size).min(99) as u8;
        if progress != last_pct {
            on_progress(progress);
            last_pct = progress;
        }
    }
    writer.flush()?;
    if downloaded != asset.size {
        bail!("downloaded {downloaded} bytes, expected {}", asset.size);
    }
    let actual: [u8; 32] = hasher.finalize().into();
    if actual != expected {
        bail!("downloaded asset failed SHA-256 verification");
    }
    on_progress(100);
    Ok(())
}

fn validate_asset(asset: &UpdateAsset) -> Result<()> {
    if asset.size == 0 || asset.size > MAX_ASSET_BYTES {
        bail!("asset size {} is outside the accepted range", asset.size);
    }
    if !is_trusted_asset_url(&asset.url) {
        bail!("refusing an update asset outside the official GitHub repository");
    }
    parse_sha256_digest(&asset.digest)?;
    Ok(())
}

fn is_trusted_asset_url(url: &str) -> bool {
    url.starts_with(RELEASE_DOWNLOAD_PREFIX)
        && !url
            .chars()
            .any(|c| c.is_ascii_control() || c.is_whitespace())
}

fn parse_sha256_digest(digest: &str) -> Result<[u8; 32]> {
    let hex = digest
        .strip_prefix("sha256:")
        .ok_or_else(|| anyhow!("release asset has no SHA-256 digest"))?;
    if hex.len() != 64 {
        bail!("release asset has a malformed SHA-256 digest");
    }
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&hex[offset..offset + 2], 16)
            .context("release asset has a malformed SHA-256 digest")?;
    }
    Ok(bytes)
}

#[cfg(test)]
fn verify_sha256(bytes: &[u8], digest: &str) -> Result<()> {
    let asset = UpdateAsset {
        url: String::new(),
        name: String::new(),
        size: bytes.len() as u64,
        digest: digest.to_owned(),
    };
    copy_verified(
        &mut std::io::Cursor::new(bytes),
        &mut std::io::sink(),
        &asset,
        &|_| {},
    )
}

fn safe_asset_file_name(name: &str) -> String {
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = leaf
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == ' ').to_string();
    if cleaned.is_empty() {
        "shiny-counter-update".to_string()
    } else {
        cleaned
    }
}

/// Open a downloaded file with the OS default handler.
///
/// On Windows we directly spawn an `.exe` instead of routing through
/// `cmd /C start` to avoid any chance of shell interpretation of the path.
/// For anything else (or non-`.exe` paths, future-proofing) we fall back to
/// the `start` shell built-in. macOS uses `open`, Linux uses `xdg-open`.
pub fn open_path(path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let is_exe = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("exe"))
            .unwrap_or(false);
        if is_exe {
            std::process::Command::new(path).spawn()?;
        } else {
            // No shell parsing - every argument is passed straight through
            // to CreateProcess, with Rust quoting any embedded whitespace.
            std::process::Command::new("explorer.exe")
                .arg(path)
                .spawn()?;
        }
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
    use std::io::Cursor;

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
                browser_download_url: format!(
                    "{RELEASE_DOWNLOAD_PREFIX}v1.2.3/ShinyCounter-1.2.3-linux-x86_64.tar.gz"
                ),
                size: 10,
                digest: Some(
                    "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
                        .into(),
                ),
            },
            AssetDto {
                name: "ShinyCounter-1.2.3-windows-x86_64.exe".into(),
                browser_download_url: format!(
                    "{RELEASE_DOWNLOAD_PREFIX}v1.2.3/ShinyCounter-1.2.3-windows-x86_64.exe"
                ),
                size: 20,
                digest: Some(
                    "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
                        .into(),
                ),
            },
            AssetDto {
                name: "ShinyCounter-1.2.3-macos-aarch64.dmg".into(),
                browser_download_url: format!(
                    "{RELEASE_DOWNLOAD_PREFIX}v1.2.3/ShinyCounter-1.2.3-macos-aarch64.dmg"
                ),
                size: 30,
                digest: Some(
                    "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
                        .into(),
                ),
            },
        ];
        let picked = pick_platform_asset(&assets);
        if let Some(s) = platform_suffix() {
            let p = picked.expect("expected an asset for this host");
            assert!(p.name.ends_with(s), "got {}", p.name);
        }
    }

    #[test]
    fn safe_asset_file_name_strips_paths_and_unsafe_chars() {
        assert_eq!(
            safe_asset_file_name("../nested\\ShinyCounter-1.2.3.exe"),
            "ShinyCounter-1.2.3.exe"
        );
        assert_eq!(safe_asset_file_name("...\u{0000}"), "_");
        assert_eq!(safe_asset_file_name("../"), "shiny-counter-update");
    }

    #[test]
    fn sha256_digest_must_match_downloaded_bytes() {
        let digest = "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert!(verify_sha256(b"hello", digest).is_ok());
        assert!(verify_sha256(b"tampered", digest).is_err());
    }

    #[test]
    fn sha256_digest_rejects_unknown_or_malformed_algorithms() {
        assert!(verify_sha256(b"hello", "sha512:abcd").is_err());
        assert!(verify_sha256(b"hello", "sha256:not-hex").is_err());
    }

    #[test]
    fn asset_validation_rejects_untrusted_or_invalid_metadata() {
        let valid = test_asset(5, valid_digest());
        assert!(validate_asset(&valid).is_ok());
        let mut maximum_size = valid.clone();
        maximum_size.size = MAX_ASSET_BYTES;
        assert!(validate_asset(&maximum_size).is_ok());

        let mut foreign_host = valid.clone();
        foreign_host.url = "https://example.com/ShinyCounter.exe".into();
        assert!(validate_asset(&foreign_host).is_err());

        let mut lookalike_path = valid.clone();
        lookalike_path.url =
            "https://github.com/DylanBricar/ShinyCounter/releases/download.evil/v1/test.bin".into();
        assert!(validate_asset(&lookalike_path).is_err());

        let mut whitespace = valid.clone();
        whitespace.url.push('\n');
        assert!(validate_asset(&whitespace).is_err());

        let mut empty = valid.clone();
        empty.size = 0;
        assert!(validate_asset(&empty).is_err());

        let mut oversized = valid.clone();
        oversized.size = MAX_ASSET_BYTES + 1;
        assert!(validate_asset(&oversized).is_err());

        let mut malformed_digest = valid;
        malformed_digest.digest = "sha256:not-a-digest".into();
        assert!(validate_asset(&malformed_digest).is_err());
    }

    #[test]
    fn production_download_writer_accepts_only_the_declared_verified_body() {
        let path = temporary_test_path("verified");
        let asset = test_asset(
            5,
            "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        );
        let mut reader = Cursor::new(b"hello");

        write_verified_download(&mut reader, &path, &asset, &|_| {})
            .expect("valid body should be persisted");

        assert_eq!(std::fs::read(&path).expect("verified file"), b"hello");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn production_download_writer_removes_short_long_and_tampered_partials() {
        let cases = [
            ("short", b"hell".as_slice(), 5, valid_digest()),
            ("long", b"hello!".as_slice(), 5, valid_digest()),
            (
                "tampered",
                b"hello".as_slice(),
                5,
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
        ];

        for (label, body, size, digest) in cases {
            let path = temporary_test_path(label);
            let asset = test_asset(size, digest);
            let mut reader = Cursor::new(body);

            assert!(write_verified_download(&mut reader, &path, &asset, &|_| {}).is_err());
            assert!(!path.exists(), "partial file survived the {label} case");
        }
    }

    fn test_asset(size: u64, digest: &str) -> UpdateAsset {
        UpdateAsset {
            url: format!("{RELEASE_DOWNLOAD_PREFIX}v1.2.3/test.bin"),
            name: "test.bin".into(),
            size,
            digest: digest.into(),
        }
    }

    fn valid_digest() -> &'static str {
        "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    }

    fn temporary_test_path(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "shiny-counter-{label}-{}-{nonce}.partial",
            std::process::id()
        ))
    }
}
