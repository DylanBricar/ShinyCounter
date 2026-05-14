use crate::types::{CaptureSource, Color};
use anyhow::{anyhow, Result};
use image::RgbaImage;
use xcap::{Monitor, Window};

#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub source: CaptureSource,
    pub label: String,
    pub short_label: String,
}

pub fn list_sources() -> Vec<SourceInfo> {
    let mut out = Vec::new();
    if let Ok(monitors) = Monitor::all() {
        for (i, m) in monitors.iter().enumerate() {
            let name = m.name().unwrap_or_else(|_| format!("monitor {i}"));
            let w = m.width().unwrap_or(0);
            let h = m.height().unwrap_or(0);
            let label = format!("[Écran {i}] {name} — {w}×{h}");
            let short_label = format!("Écran {i} ({w}×{h})");
            out.push(SourceInfo {
                source: CaptureSource::Monitor { index: i },
                label,
                short_label,
            });
        }
    }
    if let Ok(windows) = Window::all() {
        for w in windows {
            if w.is_minimized().unwrap_or(false) {
                continue;
            }
            let title = w.title().unwrap_or_default();
            if title.trim().is_empty() {
                continue;
            }
            let app = w.app_name().unwrap_or_default();
            let id = w.id().unwrap_or(0);
            let label = format!("[Fenêtre] {app} — {}", truncate(&title, 60));
            let short_label = if !app.trim().is_empty() {
                format!("Fenêtre: {}", truncate(&app, 22))
            } else {
                format!("Fenêtre: {}", truncate(&title, 22))
            };
            out.push(SourceInfo {
                source: CaptureSource::Window { id, title, app },
                label,
                short_label,
            });
        }
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{cut}…")
    }
}

pub fn capture(source: &CaptureSource) -> Result<RgbaImage> {
    match source {
        CaptureSource::Monitor { index } => capture_monitor(*index),
        CaptureSource::Window { id, title, app } => capture_window(*id, title, app),
    }
}

pub fn capture_monitor(index: usize) -> Result<RgbaImage> {
    let monitors = Monitor::all()?;
    if monitors.is_empty() {
        return Err(anyhow!("no monitors detected"));
    }
    let mon = monitors
        .get(index)
        .or_else(|| monitors.first())
        .ok_or_else(|| anyhow!("monitor {index} not available"))?;
    Ok(mon.capture_image()?)
}

pub fn capture_window(id: u32, title_hint: &str, app_hint: &str) -> Result<RgbaImage> {
    let windows = Window::all()?;
    // xcap 0.6 returns Result from accessors — collect them once and match.
    let resolved = windows
        .iter()
        .find(|w| w.id().ok() == Some(id))
        .or_else(|| {
            windows.iter().find(|w| {
                w.title().as_deref().ok() == Some(title_hint)
                    && w.app_name().as_deref().ok() == Some(app_hint)
            })
        })
        .or_else(|| {
            windows
                .iter()
                .find(|w| w.title().as_deref().ok() == Some(title_hint))
        })
        .ok_or_else(|| anyhow!("window not found (id {id}, title \"{title_hint}\")"))?;
    Ok(resolved.capture_image()?)
}

pub fn sample_color(img: &RgbaImage, x: i32, y: i32) -> Option<Color> {
    if x < 0 || y < 0 {
        return None;
    }
    let (x, y) = (x as u32, y as u32);
    if x >= img.width() || y >= img.height() {
        return None;
    }
    let p = img.get_pixel(x, y);
    Some(Color {
        r: p.0[0],
        g: p.0[1],
        b: p.0[2],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn sample_in_bounds_returns_color() {
        let mut img = RgbaImage::new(10, 10);
        img.put_pixel(3, 4, Rgba([12, 34, 56, 255]));
        assert_eq!(sample_color(&img, 3, 4), Some(Color::new(12, 34, 56)));
    }

    #[test]
    fn sample_out_of_bounds_returns_none() {
        let img = RgbaImage::new(10, 10);
        assert_eq!(sample_color(&img, -1, 0), None);
        assert_eq!(sample_color(&img, 0, -1), None);
        assert_eq!(sample_color(&img, 10, 5), None);
        assert_eq!(sample_color(&img, 5, 10), None);
    }
}
