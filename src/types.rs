use crate::i18n::Lang;
use serde::{Deserialize, Serialize};

pub const MIN_PICKERS: usize = 1;
pub const MAX_PICKERS: usize = 8;
pub const DEFAULT_INTERVAL_MS: u64 = 100;
pub const DEFAULT_TOLERANCE: u8 = 20;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    pub fn matches(self, target: Color, tolerance: u8) -> bool {
        let t = tolerance as i32;
        (self.r as i32 - target.r as i32).abs() <= t
            && (self.g as i32 - target.g as i32).abs() <= t
            && (self.b as i32 - target.b as i32).abs() <= t
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct PickerPoint {
    pub x: i32,
    pub y: i32,
    pub target: Color,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HitRecord {
    pub timestamp: String,
    pub epoch_secs: i64,
    pub delta_secs: i64,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionRecord {
    pub started_at_epoch: i64,
    pub started_at: String,
    pub ended_at_epoch: Option<i64>,
    pub ended_at: Option<String>,
    pub hits: Vec<HitRecord>,
}

impl SessionRecord {
    pub fn duration_secs(&self) -> i64 {
        let end = self.ended_at_epoch.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(self.started_at_epoch)
        });
        (end - self.started_at_epoch).max(0)
    }

    pub fn is_open(&self) -> bool {
        self.ended_at_epoch.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub pickers: Vec<PickerPoint>,
    pub tolerance: u8,
    pub interval_ms: u64,
    pub count: u32,
    pub notes: String,
    #[serde(default)]
    pub hits: Vec<HitRecord>, // legacy field, kept for migration
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
    #[serde(default)]
    pub accent_color: Option<Color>,
}

impl Preset {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            // Start with 3 pickers by default (typical shiny signature); user can
            // remove down to 1 via the per-row × button.
            pickers: vec![PickerPoint::default(); 3],
            tolerance: DEFAULT_TOLERANCE,
            interval_ms: DEFAULT_INTERVAL_MS,
            count: 0,
            notes: String::new(),
            hits: Vec::new(),
            sessions: Vec::new(),
            accent_color: None,
        }
    }

    /// Migrate legacy `hits` field into a single archived session if needed.
    pub fn migrate_hits(&mut self) {
        if self.hits.is_empty() {
            return;
        }
        // Bundle legacy hits into one closed "archive" session so the user keeps
        // their history.
        let started = self.hits.first().map(|h| h.epoch_secs).unwrap_or(0);
        let ended = self.hits.last().map(|h| h.epoch_secs).unwrap_or(started);
        let session = SessionRecord {
            started_at_epoch: started,
            started_at: String::new(),
            ended_at_epoch: Some(ended),
            ended_at: None,
            hits: std::mem::take(&mut self.hits),
        };
        // Place archive before any new sessions so chronological order is preserved.
        let mut combined = vec![session];
        combined.append(&mut self.sessions);
        self.sessions = combined;
    }

    pub fn normalize(&mut self) {
        if self.pickers.is_empty() {
            self.pickers.push(PickerPoint::default());
        } else if self.pickers.len() > MAX_PICKERS {
            self.pickers.truncate(MAX_PICKERS);
        }
        if self.interval_ms < 50 {
            self.interval_ms = 50;
        }
        self.migrate_hits();
        // Cap stored history so a long-running app cannot grow unbounded.
        const MAX_SESSIONS: usize = 500;
        const MAX_HITS_PER_SESSION: usize = 10_000;
        if self.sessions.len() > MAX_SESSIONS {
            let extra = self.sessions.len() - MAX_SESSIONS;
            self.sessions.drain(0..extra);
        }
        for s in &mut self.sessions {
            if s.hits.len() > MAX_HITS_PER_SESSION {
                let extra = s.hits.len() - MAX_HITS_PER_SESSION;
                s.hits.drain(0..extra);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LogEntry {
    pub timestamp: String,
    pub preset_name: String,
    pub count_at_event: u32,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CaptureSource {
    Monitor { index: usize },
    Window { id: u32, title: String, app: String },
}

impl Default for CaptureSource {
    fn default() -> Self {
        CaptureSource::Monitor { index: 0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub presets: Vec<Preset>,
    pub active_preset_index: usize,
    #[serde(default)]
    pub capture: CaptureSource,
    pub server_port: u16,
    pub server_enabled: bool,
    pub log: Vec<LogEntry>,
    #[serde(default)]
    pub language: Lang,
    #[serde(default)]
    pub _monitor_index_legacy: Option<usize>,
    /// When true, a newer release is downloaded automatically (no prompt).
    /// When false (default), the user gets a modal with a "Download" button.
    #[serde(default)]
    pub auto_download_updates: bool,
    /// Releases for which the user clicked "Later". Each entry mutes the
    /// prompt until `until_epoch`. Older entries are pruned on launch.
    #[serde(default)]
    pub update_snoozes: Vec<UpdateSnooze>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSnooze {
    pub version: String,
    pub until_epoch: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            presets: vec![Preset::new("Default")],
            active_preset_index: 0,
            capture: CaptureSource::default(),
            server_port: 7878,
            server_enabled: false,
            log: Vec::new(),
            language: Lang::default(),
            _monitor_index_legacy: None,
            auto_download_updates: false,
            update_snoozes: Vec::new(),
        }
    }
}

impl Config {
    pub fn ensure_invariants(&mut self) {
        if self.presets.is_empty() {
            self.presets.push(Preset::new("Default"));
        }
        for p in &mut self.presets {
            p.normalize();
        }
        if self.active_preset_index >= self.presets.len() {
            self.active_preset_index = 0;
        }
        if self.server_port == 0 {
            self.server_port = 7878;
        }
        if let Some(idx) = self._monitor_index_legacy.take() {
            self.capture = CaptureSource::Monitor { index: idx };
        }
        // Drop expired snoozes so they don't bloat the config forever.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.update_snoozes.retain(|s| s.until_epoch > now);
    }
}
