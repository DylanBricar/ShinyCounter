use crate::theme::BORDER;
use eframe::egui;
use shiny_counter::i18n::Lang;
use shiny_counter::types::{CaptureSource, Color};
use std::time::{SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;

pub(super) fn color_swatch(ui: &mut egui::Ui, c: Color, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 5.0, egui::Color32::from_rgb(c.r, c.g, c.b));
    ui.painter().rect_stroke(
        rect,
        5.0,
        egui::Stroke::new(1.0, BORDER),
        egui::StrokeKind::Inside,
    );
}

pub(super) fn short_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}...")
    }
}

pub(super) fn same_source(a: &CaptureSource, b: &CaptureSource) -> bool {
    match (a, b) {
        (CaptureSource::Monitor { index: ai }, CaptureSource::Monitor { index: bi }) => ai == bi,
        (
            CaptureSource::Window {
                id: ai,
                title: at,
                app: aa,
            },
            CaptureSource::Window {
                id: bi,
                title: bt,
                app: ba,
            },
        ) => ai == bi && (at == bt || aa == ba),
        _ => false,
    }
}

pub(super) fn epoch_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(super) fn format_local_now(lang: Lang) -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    format_datetime(now, lang)
}

pub(super) fn format_datetime(dt: OffsetDateTime, lang: Lang) -> String {
    let y = dt.year();
    let m = u8::from(dt.month());
    let d = dt.day();
    let hh = dt.hour();
    let mm = dt.minute();
    let ss = dt.second();
    match lang {
        Lang::Fr => format!("{d:02}/{m:02}/{y} {hh:02}:{mm:02}:{ss:02}"),
        Lang::En => {
            let ampm = if hh < 12 { "AM" } else { "PM" };
            let h12 = match hh % 12 {
                0 => 12,
                h => h,
            };
            format!("{}/{}/{y} {:02}:{:02}:{:02} {}", m, d, h12, mm, ss, ampm)
        }
    }
}

pub(super) fn datetime_from_epoch(epoch_secs: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(epoch_secs).unwrap_or_else(|_| OffsetDateTime::now_utc())
}

/// Write `bytes` to `path` atomically: stream to a sibling `.tmp` file, then
/// rename. Other processes reading the file (e.g. OBS Text Source) only ever
/// see the previous or the new content, never a half-written buffer.
///
/// On Windows, the rename step can fail with `ERROR_SHARING_VIOLATION` (32)
/// or `ERROR_ACCESS_DENIED` (5) if a reader (OBS, a text editor) holds an
/// open handle at the exact instant we try to swap. We retry a handful of
/// times with short backoff; OBS only keeps the file open for microseconds
/// per refresh tick so a contended write almost always succeeds on the
/// second attempt.
pub(super) fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("shiny-counter.txt");
    let tmp = parent.join(format!(".{file_name}.tmp"));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
    }

    // Retry the rename a few times on transient sharing violations. Total
    // wall budget ≈ 80 ms (5 + 10 + 20 + 40), well under one human frame so
    // the UI never visibly stalls even in the worst case.
    let mut delay_ms = 5u64;
    let mut last_err = None;
    for attempt in 0..5 {
        match std::fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) if is_transient_share_err(&e) && attempt < 4 => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                delay_ms *= 2;
            }
            Err(e) => {
                // Non-retryable, or out of attempts - clean up the temp so we
                // don't litter the user's directory with `.tmp` files.
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(last_err.unwrap_or_else(|| std::io::Error::other("rename failed after retries")))
}

/// Identify Windows sharing/access errors that are safe to retry - a reader
/// (OBS, notepad, antivirus) momentarily held the file. On other platforms
/// this is effectively a no-op since these codes don't fire there.
fn is_transient_share_err(e: &std::io::Error) -> bool {
    match e.raw_os_error() {
        // Windows: ERROR_SHARING_VIOLATION (32), ERROR_ACCESS_DENIED (5),
        // ERROR_LOCK_VIOLATION (33).
        Some(5) | Some(32) | Some(33) => true,
        _ => matches!(
            e.kind(),
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::WouldBlock
        ),
    }
}

pub(super) fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

pub(super) fn format_delta(seconds: i64) -> String {
    if seconds < 0 {
        return "0s".into();
    }
    let s = seconds as u64;
    if s < 60 {
        return format!("{s}s");
    }
    let m = s / 60;
    let rem = s % 60;
    if m < 60 {
        return format!("{m}m {rem:02}s");
    }
    let h = m / 60;
    let mm = m % 60;
    format!("{h}h {mm:02}m {rem:02}s")
}
