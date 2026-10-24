use egui::{
    Align2, Color32, CornerRadius, FontId, Painter, Rect, Response, Sense, Shape, Stroke, Ui, Vec2,
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Folder,
    FilePlus,
    Save,
    Search,
    Command,
    Terminal,
    Problems,
    Refresh,
    Branch,
    Lock,
    Image,
    Plus,
    Trash,
    Edit,
    Split,
    Eye,
    LayoutSingle,
    LayoutColumns,
    LayoutRows,
    LayoutThree,
    LayoutFour,
}

pub fn paint_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let size = rect.width().min(rect.height()).max(10.0);
    let center = rect.center();
    let stroke = Stroke::new((size * 0.085).clamp(1.1, 1.7), color);

    match icon {
        Icon::Folder => paint_folder(painter, Rect::from_center_size(center, Vec2::splat(size))),
        Icon::FilePlus => {
            let doc = Rect::from_center_size(
                center + Vec2::new(-size * 0.08, 0.0),
                Vec2::new(size * 0.52, size * 0.72),
            );
            painter.rect_stroke(doc, CornerRadius::same(2), stroke, egui::StrokeKind::Inside);
            let plus = center + Vec2::new(size * 0.28, size * 0.28);
            painter.line_segment(
                [
                    plus + Vec2::new(-size * 0.11, 0.0),
                    plus + Vec2::new(size * 0.11, 0.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    plus + Vec2::new(0.0, -size * 0.11),
                    plus + Vec2::new(0.0, size * 0.11),
                ],
                stroke,
            );
        }
        Icon::Save => {
            let outer = Rect::from_center_size(center, Vec2::splat(size * 0.68));
            painter.rect_stroke(
                outer,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_filled(
                Rect::from_min_size(
                    outer.min + Vec2::new(size * 0.16, 0.0),
                    Vec2::new(size * 0.32, size * 0.2),
                ),
                CornerRadius::same(1),
                color,
            );
            painter.rect_stroke(
                Rect::from_min_size(
                    outer.min + Vec2::new(size * 0.16, size * 0.4),
                    Vec2::new(size * 0.32, size * 0.28),
                ),
                CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Icon::Search => {
            painter.circle_stroke(
                center + Vec2::new(-size * 0.09, -size * 0.09),
                size * 0.25,
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(size * 0.1, size * 0.1),
                    center + Vec2::new(size * 0.31, size * 0.31),
                ],
                stroke,
            );
        }
        Icon::Command => {
            let r = size * 0.09;
            let offsets = [
                Vec2::new(-size * 0.2, -size * 0.2),
                Vec2::new(size * 0.2, -size * 0.2),
                Vec2::new(-size * 0.2, size * 0.2),
                Vec2::new(size * 0.2, size * 0.2),
            ];
            for offset in offsets {
                painter.circle_stroke(center + offset, r, stroke);
            }
            painter.line_segment(
                [
                    center + Vec2::new(-size * 0.2, -size * 0.2),
                    center + Vec2::new(size * 0.2, -size * 0.2),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-size * 0.2, size * 0.2),
                    center + Vec2::new(size * 0.2, size * 0.2),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-size * 0.2, -size * 0.2),
                    center + Vec2::new(-size * 0.2, size * 0.2),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(size * 0.2, -size * 0.2),
                    center + Vec2::new(size * 0.2, size * 0.2),
                ],
                stroke,
            );
        }
        Icon::Terminal => {
            let outer = Rect::from_center_size(center, Vec2::new(size * 0.82, size * 0.64));
            painter.rect_stroke(
                outer,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            let start = outer.left_top() + Vec2::new(size * 0.14, size * 0.2);
            painter.line_segment([start, start + Vec2::new(size * 0.11, size * 0.1)], stroke);
            painter.line_segment(
                [
                    start + Vec2::new(size * 0.11, size * 0.1),
                    start + Vec2::new(0.0, size * 0.2),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    outer.left_bottom() + Vec2::new(size * 0.36, -size * 0.2),
                    outer.right_bottom() + Vec2::new(-size * 0.12, -size * 0.2),
                ],
                stroke,
            );
        }
        Icon::Problems => {
            let points = vec![
                center + Vec2::new(-size * 0.32, size * 0.27),
                center + Vec2::new(0.0, -size * 0.3),
                center + Vec2::new(size * 0.32, size * 0.27),
                center + Vec2::new(-size * 0.32, size * 0.27),
            ];
            painter.add(Shape::line(points, stroke));
            painter.line_segment(
                [
                    center + Vec2::new(0.0, -size * 0.08),
                    center + Vec2::new(0.0, size * 0.09),
                ],
                stroke,
            );
            painter.circle_filled(center + Vec2::new(0.0, size * 0.19), size * 0.035, color);
        }
        Icon::Refresh => {
            let radius = size * 0.27;
            let mut points = Vec::new();
            for step in 0..=14 {
                let angle = -0.5 + (step as f32 / 14.0) * 5.2;
                points.push(center + Vec2::new(angle.cos() * radius, angle.sin() * radius));
            }
            painter.add(Shape::line(points, stroke));
            let tip = center
                + Vec2::new(
                    (-0.5 + 5.2f32).cos() * radius,
                    (-0.5 + 5.2f32).sin() * radius,
                );
            painter.line_segment([tip, tip + Vec2::new(-size * 0.12, -size * 0.02)], stroke);
            painter.line_segment([tip, tip + Vec2::new(size * 0.02, size * 0.12)], stroke);
        }
        Icon::Branch => {
            let left_x = center.x - size * 0.18;
            let right_x = center.x + size * 0.18;
            let top = center.y - size * 0.25;
            let bottom = center.y + size * 0.25;
            painter.circle_stroke(egui::pos2(left_x, top), size * 0.07, stroke);
            painter.circle_stroke(egui::pos2(left_x, bottom), size * 0.07, stroke);
            painter.circle_stroke(egui::pos2(right_x, top), size * 0.07, stroke);
            painter.line_segment(
                [
                    egui::pos2(left_x, top + size * 0.08),
                    egui::pos2(left_x, bottom - size * 0.08),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(right_x, top + size * 0.08),
                    egui::pos2(right_x, center.y),
                ],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(right_x, center.y), egui::pos2(left_x, center.y)],
                stroke,
            );
        }
        Icon::Lock => {
            let body = Rect::from_center_size(
                center + Vec2::new(0.0, size * 0.12),
                Vec2::new(size * 0.48, size * 0.38),
            );
            painter.rect_stroke(
                body,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            let mut arc = Vec::new();
            for step in 0..=10 {
                let angle = std::f32::consts::PI + (step as f32 / 10.0) * std::f32::consts::PI;
                arc.push(
                    center
                        + Vec2::new(
                            angle.cos() * size * 0.18,
                            size * 0.02 + angle.sin() * size * 0.18,
                        ),
                );
            }
            painter.add(Shape::line(arc, stroke));
        }
        Icon::Image => {
            let frame = Rect::from_center_size(center, Vec2::new(size * 0.72, size * 0.62));
            painter.rect_stroke(
                frame,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.circle_filled(
                frame.left_top() + Vec2::new(size * 0.2, size * 0.18),
                size * 0.05,
                color,
            );
            painter.add(Shape::line(
                vec![
                    frame.left_bottom() + Vec2::new(size * 0.12, -size * 0.08),
                    frame.center_bottom() + Vec2::new(0.0, -size * 0.25),
                    frame.right_bottom() + Vec2::new(-size * 0.1, -size * 0.04),
                ],
                stroke,
            ));
        }
        Icon::Plus => {
            painter.line_segment(
                [
                    center + Vec2::new(-size * 0.22, 0.0),
                    center + Vec2::new(size * 0.22, 0.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(0.0, -size * 0.22),
                    center + Vec2::new(0.0, size * 0.22),
                ],
                stroke,
            );
        }
        Icon::Edit => {
            let start = center + Vec2::new(-size * 0.25, size * 0.25);
            let end = center + Vec2::new(size * 0.22, -size * 0.22);
            painter.line_segment([start, end], stroke);
            painter.line_segment([start, start + Vec2::new(size * 0.11, size * 0.02)], stroke);
            painter.line_segment([end, end + Vec2::new(-size * 0.02, size * 0.11)], stroke);
            painter.line_segment(
                [end, center + Vec2::new(size * 0.34, -size * 0.34)],
                Stroke::new(stroke.width, color),
            );
        }
        Icon::Split => {
            let frame = Rect::from_center_size(center, Vec2::new(size * 0.7, size * 0.6));
            painter.rect_stroke(
                frame,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(frame.center().x, frame.top()),
                    egui::pos2(frame.center().x, frame.bottom()),
                ],
                stroke,
            );
        }
        Icon::Eye => {
            painter.circle_stroke(center, size * 0.27, stroke);
            painter.circle_filled(center, size * 0.085, color);
        }
        Icon::LayoutSingle => {
            let frame = Rect::from_center_size(center, Vec2::new(size * 0.7, size * 0.6));
            painter.rect_stroke(
                frame,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Icon::LayoutColumns => {
            let frame = Rect::from_center_size(center, Vec2::new(size * 0.7, size * 0.6));
            painter.rect_stroke(
                frame,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(frame.center().x, frame.top()),
                    egui::pos2(frame.center().x, frame.bottom()),
                ],
                stroke,
            );
        }
        Icon::LayoutRows => {
            let frame = Rect::from_center_size(center, Vec2::new(size * 0.7, size * 0.6));
            painter.rect_stroke(
                frame,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(frame.left(), frame.center().y),
                    egui::pos2(frame.right(), frame.center().y),
                ],
                stroke,
            );
        }
        Icon::LayoutThree => {
            let frame = Rect::from_center_size(center, Vec2::new(size * 0.7, size * 0.6));
            painter.rect_stroke(
                frame,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            for factor in [1.0 / 3.0, 2.0 / 3.0] {
                let x = frame.left() + frame.width() * factor;
                painter.line_segment(
                    [egui::pos2(x, frame.top()), egui::pos2(x, frame.bottom())],
                    stroke,
                );
            }
        }
        Icon::LayoutFour => {
            let frame = Rect::from_center_size(center, Vec2::new(size * 0.7, size * 0.6));
            painter.rect_stroke(
                frame,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(frame.center().x, frame.top()),
                    egui::pos2(frame.center().x, frame.bottom()),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(frame.left(), frame.center().y),
                    egui::pos2(frame.right(), frame.center().y),
                ],
                stroke,
            );
        }
        Icon::Trash => {
            let body = Rect::from_center_size(
                center + Vec2::new(0.0, size * 0.09),
                Vec2::new(size * 0.42, size * 0.5),
            );
            painter.rect_stroke(
                body,
                CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-size * 0.27, -size * 0.2),
                    center + Vec2::new(size * 0.27, -size * 0.2),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-size * 0.08, -size * 0.28),
                    center + Vec2::new(size * 0.08, -size * 0.28),
                ],
                stroke,
            );
        }
    }
}

pub fn icon_button(ui: &mut Ui, icon: Icon, label: &str, tooltip: &str) -> Response {
    let font = FontId::proportional(11.5);
    let galley = ui.fonts(|fonts| fonts.layout_no_wrap(label.to_owned(), font, TEXT));
    let width = 10.0 + 15.0 + 6.0 + galley.size().x + 10.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), SURFACE_HI);
    }
    let icon_rect = Rect::from_center_size(
        egui::pos2(rect.left() + 10.0 + 7.5, rect.center().y),
        Vec2::splat(15.0),
    );
    paint_icon(
        ui.painter(),
        icon_rect,
        icon,
        if response.hovered() {
            ACCENT_BRIGHT
        } else {
            MUTED
        },
    );
    let text_pos = egui::pos2(
        rect.left() + 10.0 + 15.0 + 6.0,
        rect.center().y - galley.size().y * 0.5,
    );
    ui.painter().galley(text_pos, galley, TEXT);
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

pub fn icon_only_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), SURFACE_HI);
    }
    paint_icon(
        ui.painter(),
        rect.shrink(4.0),
        icon,
        if response.hovered() { TEXT } else { MUTED },
    );
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

pub fn icon_toggle_button(ui: &mut Ui, icon: Icon, selected: bool, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), ACCENT_SOFT);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), SURFACE_HI);
    }
    paint_icon(
        ui.painter(),
        rect.shrink(4.0),
        icon,
        if selected {
            ACCENT
        } else if response.hovered() {
            TEXT
        } else {
            MUTED
        },
    );
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

pub fn paint_severity_icon(painter: &Painter, rect: Rect, severity: u8) {
    let color = match severity {
        1 => RED,
        2 => YELLOW,
        3 => BLUE,
        _ => FAINT,
    };
    paint_icon(painter, rect, Icon::Problems, color);
}
