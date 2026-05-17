use eframe::egui;

pub const BG: egui::Color32 = egui::Color32::from_rgb(13, 15, 22);
pub const SURFACE: egui::Color32 = egui::Color32::from_rgb(20, 23, 33);
pub const SURFACE_2: egui::Color32 = egui::Color32::from_rgb(28, 32, 46);
pub const BORDER: egui::Color32 = egui::Color32::from_rgb(40, 45, 64);
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(232, 234, 244);
pub const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(150, 156, 178);
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(167, 139, 250); // violet
pub const ACCENT_HOT: egui::Color32 = egui::Color32::from_rgb(196, 181, 253);
pub const SHINY: egui::Color32 = egui::Color32::from_rgb(250, 204, 21); // gold
pub const GOOD: egui::Color32 = egui::Color32::from_rgb(74, 222, 128);
pub const WARN: egui::Color32 = egui::Color32::from_rgb(250, 176, 90);
pub const BAD: egui::Color32 = egui::Color32::from_rgb(244, 114, 122);

/// Re-apply accent-derived colors on the current `Visuals`. Call once per frame
/// after [`install`] so changing the active preset takes effect immediately on
/// all stock egui widgets (buttons, combo boxes, sliders, etc.).
pub fn apply_accent(ctx: &egui::Context, accent: egui::Color32) {
    ctx.global_style_mut(|style| {
        let v = &mut style.visuals;
        let r8 = egui::CornerRadius::same(8);
        v.hyperlink_color = accent;
        v.selection.bg_fill = accent.linear_multiply(0.45);
        v.selection.stroke = egui::Stroke::new(1.0, accent);

        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent.linear_multiply(0.65));
        v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT);
        v.widgets.hovered.corner_radius = r8;

        v.widgets.active.bg_fill = accent.linear_multiply(0.85);
        v.widgets.active.weak_bg_fill = accent.linear_multiply(0.85);
        v.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent);
        v.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
        v.widgets.active.corner_radius = r8;
    });
}

pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(14.0, 11.0);
    style.spacing.window_margin = egui::Margin::same(14);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.indent = 18.0;
    style.spacing.icon_width = 20.0;
    style.spacing.interact_size = egui::vec2(46.0, 36.0);
    style.spacing.slider_rail_height = 6.0;
    style.spacing.slider_width = 200.0;
    style.spacing.combo_height = 240.0;
    style.spacing.combo_width = 220.0;
    style.spacing.text_edit_width = 200.0;

    style.text_styles = [
        (
            egui::TextStyle::Heading,
            egui::FontId::new(22.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            egui::FontId::new(14.5, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(13.5, egui::FontFamily::Monospace),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::new(14.5, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Small,
            egui::FontId::new(11.5, egui::FontFamily::Proportional),
        ),
    ]
    .into();

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.extreme_bg_color = BG;
    v.faint_bg_color = SURFACE_2;
    v.window_stroke = egui::Stroke::new(1.0, BORDER);
    v.window_corner_radius = egui::CornerRadius::same(12);
    v.menu_corner_radius = egui::CornerRadius::same(10);
    v.popup_shadow.spread = 8;
    v.popup_shadow.blur = 18;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT_HOT;
    v.selection.bg_fill = ACCENT.linear_multiply(0.45);
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT_HOT);
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);

    let r8 = egui::CornerRadius::same(8);
    v.widgets.noninteractive.bg_fill = SURFACE_2;
    v.widgets.noninteractive.weak_bg_fill = SURFACE_2;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_DIM);
    v.widgets.noninteractive.corner_radius = r8;

    v.widgets.inactive.bg_fill = SURFACE_2;
    v.widgets.inactive.weak_bg_fill = SURFACE_2;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    v.widgets.inactive.corner_radius = r8;

    v.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 50, 72);
    v.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(40, 44, 64);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT.linear_multiply(0.65));
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT);
    v.widgets.hovered.corner_radius = r8;

    v.widgets.active.bg_fill = ACCENT.linear_multiply(0.85);
    v.widgets.active.weak_bg_fill = ACCENT.linear_multiply(0.85);
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT_HOT);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    v.widgets.active.corner_radius = r8;

    v.widgets.open.bg_fill = SURFACE_2;
    v.widgets.open.weak_bg_fill = SURFACE_2;
    v.widgets.open.bg_stroke = egui::Stroke::new(1.0, BORDER);
    v.widgets.open.fg_stroke = egui::Stroke::new(1.0, TEXT);
    v.widgets.open.corner_radius = r8;

    ctx.set_global_style(style);
    ctx.set_visuals(v);
}

pub fn card<R>(ui: &mut egui::Ui, inner: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::NONE
        .fill(SURFACE)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::same(16))
        .show(ui, inner)
        .inner
}

pub fn pill(ui: &mut egui::Ui, label: &str, bg: egui::Color32, fg: egui::Color32) {
    pill_inner(ui, label, bg, fg, false);
}

pub fn pill_dot(ui: &mut egui::Ui, label: &str, bg: egui::Color32, fg: egui::Color32) {
    pill_inner(ui, label, bg, fg, true);
}

fn pill_inner(
    ui: &mut egui::Ui,
    label: &str,
    bg: egui::Color32,
    _fg: egui::Color32,
    with_dot: bool,
) {
    let font_id = egui::FontId::proportional(12.0);
    // Pick a high-contrast text color so the pill is always legible regardless of
    // the accent that drove it.
    let text_color = contrast_text(bg);
    let text_w = ui.fonts_mut(|f| {
        f.layout_no_wrap(label.to_string(), font_id.clone(), text_color)
            .size()
            .x
    });
    let dot_w = if with_dot { 14.0 } else { 0.0 };
    let h = 22.0;
    let pad_x = 12.0;
    let w = (text_w + dot_w + pad_x * 2.0).ceil();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter().rect_filled(rect, h * 0.5, bg);
    ui.painter().rect_stroke(
        rect,
        h * 0.5,
        egui::Stroke::new(1.0, bg.linear_multiply(0.7)),
        egui::StrokeKind::Inside,
    );
    let mut x = rect.left() + pad_x;
    if with_dot {
        let dot_center = egui::pos2(x + 4.0, rect.center().y);
        let dot_color = if text_color == egui::Color32::WHITE {
            egui::Color32::WHITE
        } else {
            egui::Color32::BLACK
        };
        ui.painter().circle_filled(dot_center, 4.0, dot_color);
        x += 12.0;
    }
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        font_id,
        text_color,
    );
}

/// Picks black or white text depending on the perceptual brightness of `bg`.
fn contrast_text(bg: egui::Color32) -> egui::Color32 {
    let [r, g, b, _] = bg.to_array();
    let lum = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
    if lum > 140.0 {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    }
}

#[allow(dead_code)]
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    colored_button(ui, label, ACCENT, ACCENT_HOT)
}

pub fn colored_button(
    ui: &mut egui::Ui,
    label: &str,
    fill: egui::Color32,
    stroke_color: egui::Color32,
) -> egui::Response {
    let text = egui::RichText::new(label)
        .color(egui::Color32::WHITE)
        .size(15.0)
        .strong();
    let btn = egui::Button::new(text)
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke_color))
        .corner_radius(egui::CornerRadius::same(10))
        .min_size(egui::vec2(0.0, 36.0));
    let resp = ui.add(btn);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn ghost_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let text = egui::RichText::new(label).color(TEXT).size(14.0);
    let btn = egui::Button::new(text)
        .fill(SURFACE_2)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(10))
        .min_size(egui::vec2(0.0, 32.0));
    let resp = ui.add(btn);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

#[allow(dead_code)]
pub fn danger_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let text = egui::RichText::new(label)
        .color(egui::Color32::WHITE)
        .size(14.0);
    let btn = egui::Button::new(text)
        .fill(BAD.linear_multiply(0.85))
        .stroke(egui::Stroke::new(1.0, BAD))
        .corner_radius(egui::CornerRadius::same(10))
        .min_size(egui::vec2(0.0, 32.0));
    let resp = ui.add(btn);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Small square icon button (e.g. an "x" close-style button).
pub fn icon_button(ui: &mut egui::Ui, glyph: &str, fg: egui::Color32) -> egui::Response {
    let text = egui::RichText::new(glyph).color(fg).strong().size(16.0);
    let btn = egui::Button::new(text)
        .fill(BG)
        .stroke(egui::Stroke::new(1.0, fg.linear_multiply(0.55)))
        .corner_radius(egui::CornerRadius::same(8))
        .min_size(egui::vec2(32.0, 30.0));
    let resp = ui.add(btn);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Render a small "ⓘ" / `?` info bubble that shows a tooltip on hover.
pub fn info_icon(ui: &mut egui::Ui, tip: &str) {
    let size = egui::vec2(18.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    let hovered = response.hovered();
    let bg = if hovered {
        ACCENT
    } else {
        egui::Color32::from_rgb(70, 78, 110)
    };
    painter.circle_filled(rect.center(), 8.5, bg.linear_multiply(0.55));
    painter.circle_stroke(rect.center(), 8.5, egui::Stroke::new(1.0, bg));
    painter.text(
        rect.center() + egui::vec2(0.0, 0.5),
        egui::Align2::CENTER_CENTER,
        "i",
        egui::FontId::new(12.0, egui::FontFamily::Proportional),
        egui::Color32::WHITE,
    );
    response.on_hover_text(tip);
}

/// Draw a stylised Pokéball at the given center.
pub fn paint_pokeball(painter: &egui::Painter, center: egui::Pos2, radius: f32) {
    use std::f32::consts::PI;
    let red = egui::Color32::from_rgb(232, 65, 65);
    let white = egui::Color32::from_rgb(248, 248, 248);
    let black = egui::Color32::from_rgb(20, 20, 20);

    // Bottom half (white background)
    painter.circle_filled(center, radius, white);

    // Top half (red) - polygon approximating the upper half.
    let n = 40;
    let mut pts: Vec<egui::Pos2> = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let angle = PI + PI * t; // pi -> 2pi  (top half in screen coords)
        pts.push(egui::pos2(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    painter.add(egui::Shape::convex_polygon(pts, red, egui::Stroke::NONE));

    // Equatorial black band - slightly inset so it sits inside the circle.
    let band_half_w = radius * 0.97;
    let band_h = (radius * 0.20).max(2.0);
    painter.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(center.x - band_half_w, center.y - band_h * 0.5),
            egui::pos2(center.x + band_half_w, center.y + band_h * 0.5),
        ),
        0.0,
        black,
    );

    // Outer ring
    painter.circle_stroke(center, radius, egui::Stroke::new(1.4, black));

    // Central button
    let btn_r = radius * 0.32;
    painter.circle_filled(center, btn_r, white);
    painter.circle_stroke(center, btn_r, egui::Stroke::new(1.4, black));
    painter.circle_stroke(center, btn_r * 0.45, egui::Stroke::new(1.0, black));
}
