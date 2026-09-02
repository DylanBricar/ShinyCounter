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
            let label = format!("[Écran {i}] {name} - {w}x{h}");
            let short_label = format!("Écran {i} ({w}x{h})");
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
            let label = format!("[Fenêtre] {app} - {}", truncate(&title, 60));
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
        format!("{cut}...")
    }
}

/// Full-screen capture — used only for the picker UI (one-shot, not hot path).
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
    let resolved = windows
        .iter()
        .find(|w| {
            w.id().ok() == Some(id)
                && (field_matches(w.app_name(), app_hint) || field_matches(w.title(), title_hint))
        })
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

/// A frame cropped around every configured picker. Coordinates passed to
/// `color_at` remain relative to the original source, so callers do not need
/// to know whether the monitor capture was cropped.
pub(crate) struct SamplingFrame {
    image: RgbaImage,
    origin_x: i32,
    origin_y: i32,
}

impl SamplingFrame {
    pub(crate) fn color_at(&self, x: i32, y: i32) -> Option<Color> {
        sample_color(
            &self.image,
            x.checked_sub(self.origin_x)?,
            y.checked_sub(self.origin_y)?,
        )
    }
}

/// Capture only the smallest monitor rectangle containing all valid picker
/// coordinates. Windows without partial-capture support and macOS Retina
/// displays use the complete frame to preserve screenshot pixel coordinates.
pub(crate) fn capture_for_sampling<I>(source: &CaptureSource, points: I) -> Result<SamplingFrame>
where
    I: IntoIterator<Item = (i32, i32)>,
{
    match source {
        CaptureSource::Monitor { index } => {
            let monitors = Monitor::all()?;
            let mon = monitors
                .get(*index)
                .or_else(|| monitors.first())
                .ok_or_else(|| anyhow!("monitor {index} not available"))?;
            // On macOS, xcap reports monitor bounds in logical points while
            // the screenshot can contain Retina-resolution pixels. Picker
            // coordinates are screenshot pixels, so cropping without an
            // explicit scale conversion would sample the wrong location.
            #[cfg(target_os = "macos")]
            {
                let _ = points;
                Ok(SamplingFrame {
                    image: mon.capture_image()?,
                    origin_x: 0,
                    origin_y: 0,
                })
            }
            #[cfg(not(target_os = "macos"))]
            {
                let width = mon.width()?;
                let height = mon.height()?;
                let Some((x, y, region_width, region_height)) =
                    sampling_bounds(points, width, height)
                else {
                    return Ok(SamplingFrame {
                        image: RgbaImage::new(0, 0),
                        origin_x: 0,
                        origin_y: 0,
                    });
                };
                Ok(SamplingFrame {
                    image: mon.capture_region(x, y, region_width, region_height)?,
                    origin_x: x as i32,
                    origin_y: y as i32,
                })
            }
        }
        CaptureSource::Window { id, title, app } => Ok(SamplingFrame {
            image: capture_window(*id, title, app)?,
            origin_x: 0,
            origin_y: 0,
        }),
    }
}

#[cfg(any(not(target_os = "macos"), test))]
fn sampling_bounds<I>(points: I, width: u32, height: u32) -> Option<(u32, u32, u32, u32)>
where
    I: IntoIterator<Item = (i32, i32)>,
{
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (x, y) in points {
        if x < 0 || y < 0 {
            continue;
        }
        let (x, y) = (x as u32, y as u32);
        if x >= width || y >= height {
            continue;
        }
        bounds = Some(match bounds {
            Some((min_x, min_y, max_x, max_y)) => {
                (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
            }
            None => (x, y, x, y),
        });
    }
    bounds.map(|(min_x, min_y, max_x, max_y)| (min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
}

fn field_matches<E>(value: Result<String, E>, hint: &str) -> bool {
    !hint.trim().is_empty() && value.as_deref().ok() == Some(hint)
}

/// Sample a single pixel from a source without allocating a full-screen image.
/// On monitors: uses `capture_region(x, y, 1, 1)` — only transfers 4 bytes.
/// On windows: captures the full window then reads the pixel (window capture
/// doesn't support partial regions in xcap).
pub fn sample_pixel(source: &CaptureSource, x: i32, y: i32) -> Result<Option<Color>> {
    if x < 0 || y < 0 {
        return Ok(None);
    }
    let (ux, uy) = (x as u32, y as u32);
    match source {
        CaptureSource::Monitor { index } => {
            let monitors = Monitor::all()?;
            let mon = monitors
                .get(*index)
                .or_else(|| monitors.first())
                .ok_or_else(|| anyhow!("monitor {index} not available"))?;
            let w = mon.width().unwrap_or(0);
            let h = mon.height().unwrap_or(0);
            if ux >= w || uy >= h {
                return Ok(None);
            }
            // 1×1 region — only 4 bytes transferred from the GPU/framebuffer.
            let img = mon.capture_region(ux, uy, 1, 1)?;
            Ok(img.get_pixel_checked(0, 0).map(|p| Color {
                r: p.0[0],
                g: p.0[1],
                b: p.0[2],
            }))
        }
        CaptureSource::Window { id, title, app } => {
            // Window capture doesn't support partial regions in xcap 0.6;
            // capture the full window and read the pixel.
            let img = capture_window(*id, title, app)?;
            Ok(sample_color(&img, x, y))
        }
    }
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

    #[test]
    fn sampling_bounds_crop_to_valid_picker_coordinates() {
        assert_eq!(
            sampling_bounds([(-1, 0), (7, 8), (2, 3), (20, 2)], 10, 10),
            Some((2, 3, 6, 6))
        );
        assert_eq!(sampling_bounds([(-1, 0), (10, 2)], 10, 10), None);
    }

    #[test]
    fn cropped_frame_preserves_source_coordinates() {
        let mut image = RgbaImage::new(2, 2);
        image.put_pixel(1, 1, Rgba([9, 8, 7, 255]));
        let frame = SamplingFrame {
            image,
            origin_x: 5,
            origin_y: 7,
        };

        assert_eq!(frame.color_at(6, 8), Some(Color::new(9, 8, 7)));
        assert_eq!(frame.color_at(4, 8), None);
        assert_eq!(frame.color_at(i32::MIN, i32::MIN), None);
    }
}
