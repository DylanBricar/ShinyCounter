use crate::types::Config;
use anyhow::{Context, Result};
use std::path::PathBuf;

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
    match load_inner() {
        Ok(mut cfg) => {
            cfg.ensure_invariants();
            cfg
        }
        Err(_) => Config::default(),
    }
}

fn load_inner() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(&path).with_context(|| format!("reading {path:?}"))?;
    let cfg: Config = serde_json::from_str(&raw).context("parsing config JSON")?;
    Ok(cfg)
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path()?;
    let json = serde_json::to_string_pretty(cfg)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Preset;

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
}
