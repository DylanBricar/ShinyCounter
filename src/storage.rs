use crate::types::Config;
use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct LoadOutcome {
    pub config: Config,
    pub warning: Option<String>,
}

pub fn config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("could not resolve OS config directory")?
        .join("ShinyCounter");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {dir:?}"))?;
    Ok(dir)
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

pub fn load() -> Config {
    load_with_warning().config
}

pub fn load_with_warning() -> LoadOutcome {
    match config_path() {
        Ok(path) => load_or_default_from_path(&path),
        Err(e) => LoadOutcome {
            config: Config::default(),
            warning: Some(format!("could not resolve config path: {e}")),
        },
    }
}

fn load_or_default_from_path(path: &Path) -> LoadOutcome {
    match load_inner_from_path(path) {
        Ok(mut cfg) => {
            cfg.ensure_invariants();
            LoadOutcome {
                config: cfg,
                warning: None,
            }
        }
        Err(e) => {
            let backup_note = match backup_invalid_config(path) {
                Ok(Some(backup)) => format!("; backup saved to {}", backup.display()),
                Ok(None) => String::new(),
                Err(backup_err) => format!("; backup failed: {backup_err}"),
            };
            LoadOutcome {
                config: Config::default(),
                warning: Some(format!(
                    "could not load config at {}: {e}{backup_note}",
                    path.display()
                )),
            }
        }
    }
}

fn load_inner_from_path(path: &Path) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(path).with_context(|| format!("reading {path:?}"))?;
    let cfg: Config = serde_json::from_str(&raw).context("parsing config JSON")?;
    Ok(cfg)
}

fn backup_invalid_config(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = path.with_extension(format!("json.invalid-{stamp}.bak"));
    std::fs::copy(path, &backup)
        .with_context(|| format!("copying invalid config to {}", backup.display()))?;
    Ok(Some(backup))
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path()?;
    save_to_path(cfg, &path)
}

fn save_to_path(cfg: &Config, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(cfg)?;
    let tmp = path.with_extension("json.tmp");
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)
            .with_context(|| format!("creating temporary config {}", tmp.display()))?;
        file.write_all(json.as_bytes())
            .with_context(|| format!("writing temporary config {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("syncing temporary config {}", tmp.display()))?;
        drop(file);
        std::fs::rename(&tmp, path)
            .with_context(|| format!("replacing config {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Preset;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn round_trip_json() {
        let mut cfg = Config::default();
        cfg.presets.push(Preset::new("Wailord Switch"));
        cfg.active_preset_index = 1;
        let raw = serde_json::to_string(&cfg).unwrap();
        let back: Config = serde_json::from_str(&raw).unwrap();
        assert_eq!(back.presets.len(), 2);
        assert_eq!(back.active_preset_index, 1);
        assert_eq!(back.presets[1].name, "Wailord Switch");
    }

    #[test]
    fn malformed_config_is_backed_up_before_defaulting() {
        let dir = unique_test_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, "{ not valid json").unwrap();

        let loaded = load_or_default_from_path(&path);

        assert_eq!(loaded.config.presets.len(), 1);
        assert!(loaded.warning.is_some());
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().contains("invalid"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&backups[0]).unwrap(),
            "{ not valid json"
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn saving_replaces_an_existing_config_atomically() {
        let dir = unique_test_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, "old config").unwrap();
        let mut cfg = Config::default();
        cfg.presets[0].name = "Updated".into();

        save_to_path(&cfg, &path).expect("existing config should be replaced");

        let loaded = load_inner_from_path(&path).expect("replacement should be valid JSON");
        assert_eq!(loaded.presets[0].name, "Updated");
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "shiny-counter-storage-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
