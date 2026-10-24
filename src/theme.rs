use egui::{
    Align2, Color32, CornerRadius, FontId, Painter, Rect, Response, Sense, Stroke, Ui, Vec2,
};

// Brand palette
pub const ACCENT: Color32 = Color32::from_rgb(0xE8, 0xA7, 0x5D);
pub const ACCENT_BRIGHT: Color32 = Color32::from_rgb(0xF6, 0xC1, 0x77);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x33, 0x2A, 0x1D);

// Surfaces
pub const BG: Color32 = Color32::from_rgb(0x15, 0x16, 0x1A);
pub const SURFACE: Color32 = Color32::from_rgb(0x1B, 0x1D, 0x22);
pub const SURFACE_HI: Color32 = Color32::from_rgb(0x24, 0x26, 0x2D);
pub const SIDEBAR_BG: Color32 = Color32::from_rgb(0x18, 0x19, 0x1E);
pub const TOPBAR_BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x19);
pub const TABBAR_BG: Color32 = Color32::from_rgb(0x12, 0x13, 0x17);
pub const STATUS_BG: Color32 = Color32::from_rgb(0x18, 0x19, 0x1E);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x18, 0x19, 0x1E);
pub const GUTTER_BG: Color32 = Color32::from_rgb(0x18, 0x19, 0x1E);

// Lines and text
pub const BORDER: Color32 = Color32::from_rgb(0x2A, 0x2C, 0x33);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x37, 0x3A, 0x43);
pub const TEXT: Color32 = Color32::from_rgb(0xD7, 0xD9, 0xE0);
pub const MUTED: Color32 = Color32::from_rgb(0x8A, 0x8F, 0x9A);
pub const FAINT: Color32 = Color32::from_rgb(0x5C, 0x61, 0x6C);

// Semantic colors
pub const RED: Color32 = Color32::from_rgb(0xF2, 0x6D, 0x6D);
pub const RED_BG: Color32 = Color32::from_rgb(0x3A, 0x22, 0x22);
pub const YELLOW: Color32 = Color32::from_rgb(0xE5, 0xC0, 0x7B);
pub const YELLOW_BG: Color32 = Color32::from_rgb(0x3A, 0x32, 0x20);
pub const BLUE: Color32 = Color32::from_rgb(0x61, 0xAF, 0xEF);
pub const GREEN: Color32 = Color32::from_rgb(0x98, 0xC3, 0x79);
pub const PURPLE: Color32 = Color32::from_rgb(0xC6, 0x78, 0xDD);

pub fn paint_brand_mark(painter: &Painter, rect: Rect) {
    let radius = (rect.height() * 0.22).clamp(4.0, 14.0) as u8;
    painter.rect_filled(rect, CornerRadius::same(radius), ACCENT);
    painter.text(
        rect.center() + Vec2::new(0.0, -0.5),
        Align2::CENTER_CENTER,
        "K",
        FontId::proportional(rect.height() * 0.62),
        Color32::from_rgb(0x1A, 0x14, 0x0A),
    );
}

pub fn status_dot(ui: &mut Ui, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.0, color);
    response
}

pub fn paint_chevron(painter: &Painter, center: egui::Pos2, expanded: bool, color: Color32) {
    let stroke = Stroke::new(1.3, color);
    let size = 3.5;
    if expanded {
        painter.line_segment(
            [
                center + Vec2::new(-size, -size * 0.45),
                center + Vec2::new(0.0, size * 0.45),
            ],
            stroke,
        );
        painter.line_segment(
            [
                center + Vec2::new(0.0, size * 0.45),
                center + Vec2::new(size, -size * 0.45),
            ],
            stroke,
        );
    } else {
        painter.line_segment(
            [
                center + Vec2::new(-size * 0.45, -size),
                center + Vec2::new(size * 0.55, 0.0),
            ],
            stroke,
        );
        painter.line_segment(
            [
                center + Vec2::new(size * 0.55, 0.0),
                center + Vec2::new(-size * 0.45, size),
            ],
            stroke,
        );
    }
}

pub fn paint_folder(painter: &Painter, rect: Rect) {
    let body = Rect::from_min_size(rect.left_top() + Vec2::new(1.0, 4.0), Vec2::new(14.0, 9.0));
    let tab = Rect::from_min_size(rect.left_top() + Vec2::new(1.0, 2.0), Vec2::new(7.0, 3.5));
    painter.rect_filled(body, CornerRadius::same(2), ACCENT.gamma_multiply(0.9));
    painter.rect_filled(
        tab,
        CornerRadius::same(1),
        ACCENT_BRIGHT.gamma_multiply(0.9),
    );
}

pub fn paint_file_badge(painter: &Painter, rect: Rect, extension: &str) {
    let color = extension_color(extension);
    let background = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 34);
    painter.rect_filled(rect, CornerRadius::same(3), background);

    if extension.is_empty() {
        let stroke = Stroke::new(1.0, color);
        let left = rect.left() + rect.width() * 0.3;
        let right = rect.right() - rect.width() * 0.3;
        painter.line_segment(
            [
                egui::pos2(left, rect.center().y - 2.0),
                egui::pos2(right, rect.center().y - 2.0),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(left, rect.center().y + 2.0),
                egui::pos2(right, rect.center().y + 2.0),
            ],
            stroke,
        );
        return;
    }

    let label = extension.chars().take(3).collect::<String>().to_uppercase();
    let font = FontId::monospace((rect.height() * 0.56).max(7.0));
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, color);
}

pub fn extension_color(extension: &str) -> Color32 {
    match extension.to_ascii_lowercase().as_str() {
        "rs" => ACCENT,
        "toml" => PURPLE,
        "md" | "markdown" => BLUE,
        "json" => YELLOW,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tif" | "tiff" | "svg" => PURPLE,
        "sh" | "zsh" | "bash" => GREEN,
        "html" | "htm" => Color32::from_rgb(0xF7, 0x76, 0x8E),
        "css" | "scss" => Color32::from_rgb(0x7D, 0xCF, 0xFF),
        "yml" | "yaml" => Color32::from_rgb(0x9E, 0xCE, 0x6A),
        "lock" => MUTED,
        _ => MUTED,
    }
}

pub fn paint_close_x(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.2, color);
    painter.line_segment([rect.left_top(), rect.right_bottom()], stroke);
    painter.line_segment([rect.right_top(), rect.left_bottom()], stroke);
}

pub fn close_button(ui: &mut Ui) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
    let color = if response.hovered() { TEXT } else { FAINT };
    paint_close_x(ui.painter(), rect.shrink(5.5), color);
    response
}

pub fn chip(ui: &mut Ui, text: &str, color: Color32, background: Color32) -> Response {
    let font = FontId::proportional(10.0);
    let galley = ui.fonts(|fonts| fonts.layout_no_wrap(text.to_owned(), font, color));
    let size = Vec2::new(galley.size().x + 12.0, 17.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), background);
    let text_pos = rect.center() - galley.size() / 2.0;
    ui.painter().galley(text_pos, galley, color);
    response
}

pub fn key_chip(ui: &mut Ui, text: &str) {
    let font = FontId::monospace(9.5);
    let galley = ui.fonts(|fonts| fonts.layout_no_wrap(text.to_owned(), font, MUTED));
    let size = Vec2::new(galley.size().x + 12.0, 18.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), SURFACE_HI);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, BORDER),
        egui::StrokeKind::Inside,
    );
    let text_pos = rect.center() - galley.size() / 2.0;
    ui.painter().galley(text_pos, galley, MUTED);
}

pub fn paint_dot_grid(ui: &Ui) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    let color = Color32::from_white_alpha(6);
    let step = 30.0;
    let mut y = rect.top() + 15.0;
    while y < rect.bottom() {
        let mut x = rect.left() + 15.0;
        while x < rect.right() {
            painter.circle_filled(egui::pos2(x, y), 1.0, color);
            x += step;
        }
        y += step;
    }
}
