use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Margin, Rect, RichText, ScrollArea, Sense, Stroke,
    TextEdit, Vec2,
};

use crate::app::{
    char_index_for_lsp, hash_str, line_of_char, severity_color, severity_label, KodmozCode,
};
use crate::highlight::LanguageId;
use crate::theme;

impl KodmozCode {
    pub(crate) fn show_tab_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        if index >= self.tabs.len() {
            return;
        }

        if self.tabs[index].language.supports_preview() {
            match self.tabs[index].view_mode {
                crate::app::TabViewMode::Preview => {
                    self.preview_ui(ui, index);
                    return;
                }
                crate::app::TabViewMode::Split => {
                    let available = ui.available_width();
                    if available < 360.0 {
                        self.preview_ui(ui, index);
                        return;
                    }
                    let tab_id = self.tabs[index].id;
                    egui::SidePanel::right(egui::Id::new(("kodmoz_preview_split", tab_id)))
                        .resizable(true)
                        .default_width((available * 0.5).max(160.0))
                        .min_width(140.0)
                        .max_width((available - 140.0).max(140.0))
                        .frame(
                            egui::Frame::NONE
                                .fill(theme::SURFACE)
                                .inner_margin(Margin::symmetric(10, 8)),
                        )
                        .show_inside(ui, |ui| self.preview_contents(ui, index, true));
                    self.text_editor_ui(ui, index);
                    return;
                }
                crate::app::TabViewMode::Edit => {}
            }
        }

        match &self.tabs[index].language {
            LanguageId::Image => {
                self.image_ui(ui, index);
                return;
            }
            LanguageId::Binary => {
                self.binary_ui(ui, index);
                return;
            }
            _ => {}
        }
        self.text_editor_ui(ui, index);
    }

    fn text_editor_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        if index >= self.tabs.len() {
            return;
        }

        let tab_id = self.tabs[index].id;
        let language = self.tabs[index].language.clone();
        let font = FontId::monospace(self.font_size);
        let row_height = ui.fonts(|fonts| fonts.row_height(&font));
        let mut content = std::mem::take(&mut self.tabs[index].content);
        let text_id = Id::new(("kodmoz_editor", tab_id));

        // Jumping from the problem list: move the cursor and scroll before
        // the text edit computes its layout.
        if let Some(char_index) = self.tabs[index].pending_cursor.take() {
            let mut state =
                egui::text_edit::TextEditState::load(ui.ctx(), text_id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(char_index),
                )));
            state.store(ui.ctx(), text_id);
            ui.memory_mut(|memory| memory.request_focus(text_id));
            if self.tabs[index].pending_scroll.is_none() {
                let line = line_of_char(&content, char_index);
                self.tabs[index].pending_scroll =
                    Some(((line as f32) * row_height - 96.0).max(0.0));
            }
        }
        let pending_scroll = self.tabs[index].pending_scroll.take();

        let line_count = content.matches('\n').count() + 1;
        let digits = line_count.max(9).to_string().len().max(3);
        let digit_width = ui.fonts(|fonts| {
            fonts
                .layout_no_wrap("0".to_owned(), font.clone(), Color32::WHITE)
                .size()
                .x
        });
        let gutter_width = digits as f32 * digit_width + 26.0;
        let viewport_height = ui.available_height();
        let desired_rows = ((viewport_height / row_height).ceil() as usize).max(12) + 2;

        let highlighter = &mut self.highlighter;
        let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
            let job = highlighter.highlight(text, &language, font.clone(), wrap_width);
            ui.fonts(|fonts| fonts.layout_job(job))
        };

        let mut editor_output = None;
        let mut gutter_rect = None;

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(gutter_width, viewport_height), Sense::hover());
            gutter_rect = Some(rect);

            let mut scroll = ScrollArea::vertical()
                .id_salt(("kodmoz_editor_scroll", tab_id))
                .auto_shrink([false, false]);
            if let Some(offset) = pending_scroll {
                scroll = scroll.vertical_scroll_offset(offset);
            }

            let output = scroll.show(ui, |ui| {
                ui.set_width(ui.available_width());
                TextEdit::multiline(&mut content)
                    .id(text_id)
                    .code_editor()
                    .frame(false)
                    .desired_width(f32::INFINITY)
                    .desired_rows(desired_rows)
                    .margin(Margin::symmetric(12, 8))
                    .layouter(&mut layouter)
                    .show(ui)
            });
            editor_output = Some(output);
        });

        let changed = editor_output
            .as_ref()
            .map(|output| output.inner.response.changed())
            .unwrap_or(false);
        let cursor_char = editor_output
            .as_ref()
            .and_then(|output| output.inner.cursor_range)
            .map(|range| range.primary.ccursor.index);

        self.tabs[index].content = content;
        if let Some(cursor_char) = cursor_char {
            self.tabs[index].cursor_char = cursor_char;
        }
        if changed {
            let hash = hash_str(&self.tabs[index].content);
            let tab = &mut self.tabs[index];
            tab.dirty = hash != tab.saved_hash;
            tab.edit_time = Some(Instant::now());
        }

        let diagnostics: Vec<crate::lsp::DiagnosticItem> = self.tabs[index]
            .path
            .as_ref()
            .and_then(|path| self.diagnostics.get(path))
            .cloned()
            .unwrap_or_default();
        let mut diagnostics_by_line: HashMap<u32, u8> = HashMap::new();
        for diagnostic in &diagnostics {
            let severity = diagnostics_by_line
                .entry(diagnostic.line)
                .or_insert(diagnostic.severity);
            if diagnostic.severity < *severity {
                *severity = diagnostic.severity;
            }
        }

        if let (Some(output), Some(gutter_rect)) = (&editor_output, gutter_rect) {
            let text_output = &output.inner;
            let galley = &text_output.galley;
            let origin = text_output.galley_pos;

            let painter = ui.painter().with_clip_rect(gutter_rect);
            painter.rect_filled(gutter_rect, CornerRadius::ZERO, theme::GUTTER_BG);
            painter.line_segment(
                [
                    egui::pos2(gutter_rect.right() - 0.5, gutter_rect.top()),
                    egui::pos2(gutter_rect.right() - 0.5, gutter_rect.bottom()),
                ],
                Stroke::new(1.0, theme::BORDER),
            );

            let cursor_line = line_of_char(&self.tabs[index].content, self.tabs[index].cursor_char);
            let mut line_number = 1usize;
            let mut first_row_of_line = true;
            for row in &galley.rows {
                let y = origin.y + row.rect.min.y;
                if y + row.rect.height() >= gutter_rect.top() && y <= gutter_rect.bottom() {
                    let active = line_number - 1 == cursor_line;
                    if active {
                        painter.rect_filled(
                            Rect::from_min_size(
                                egui::pos2(gutter_rect.left(), y),
                                Vec2::new(2.0, row.rect.height()),
                            ),
                            CornerRadius::ZERO,
                            theme::ACCENT,
                        );
                    }
                    painter.text(
                        egui::pos2(gutter_rect.right() - 10.0, y),
                        Align2::RIGHT_TOP,
                        line_number,
                        font.clone(),
                        if active { theme::ACCENT } else { theme::FAINT },
                    );

                    if first_row_of_line {
                        if let Some(severity) =
                            diagnostics_by_line.get(&(line_number.saturating_sub(1) as u32))
                        {
                            painter.circle_filled(
                                egui::pos2(gutter_rect.right() - 3.5, y + row.rect.height() * 0.5),
                                2.6,
                                severity_color(*severity),
                            );
                        }
                    }
                }
                if row.ends_with_newline {
                    line_number += 1;
                    first_row_of_line = true;
                } else {
                    first_row_of_line = false;
                }
            }

            if !diagnostics.is_empty() {
                let text_clip = text_output.text_clip_rect.intersect(ui.clip_rect());
                let squiggle_painter = ui.painter().with_clip_rect(text_clip);

                let mut row_ranges = Vec::with_capacity(galley.rows.len());
                let mut row_start_char = 0usize;
                for row in &galley.rows {
                    let mut row_end_char = row_start_char + row.glyphs.len();
                    if row.ends_with_newline {
                        row_end_char += 1;
                    }
                    row_ranges.push((row_start_char, row_end_char));
                    row_start_char = row_end_char;
                }

                let content = &self.tabs[index].content;
                for (diagnostic_index, diagnostic) in diagnostics.iter().enumerate() {
                    let start = char_index_for_lsp(
                        content,
                        diagnostic.line as usize,
                        diagnostic.col as usize,
                    );
                    let mut end = char_index_for_lsp(
                        content,
                        diagnostic.end_line as usize,
                        diagnostic.end_col as usize,
                    );
                    if end <= start {
                        end = start + 1;
                    }
                    let color = severity_color(diagnostic.severity);

                    for (row_index, (row_start, row_end)) in row_ranges.iter().enumerate() {
                        if *row_end <= start || *row_start >= end {
                            continue;
                        }
                        let row = &galley.rows[row_index];
                        let local_start_char = start.max(*row_start);
                        let mut local_end_char = end.min(*row_end);
                        if local_end_char <= local_start_char {
                            local_end_char = (local_start_char + 1).min(*row_end);
                        }
                        let local_start = local_start_char - *row_start;
                        let local_end = local_end_char - *row_start;
                        let start_x = row
                            .glyphs
                            .get(local_start)
                            .map(|glyph| glyph.pos.x)
                            .unwrap_or(row.rect.max.x);
                        let end_x = row
                            .glyphs
                            .get(local_end)
                            .map(|glyph| glyph.pos.x)
                            .unwrap_or(row.rect.max.x);
                        let x0 = origin.x + start_x;
                        let x1 = origin.x + end_x;
                        let x1 = if x1 <= x0 + 2.0 { x0 + 4.0 } else { x1 };
                        let y = origin.y + row.rect.max.y - 2.0;
                        if y < text_clip.top() || y > text_clip.bottom() {
                            continue;
                        }
                        squiggle_painter.add(egui::Shape::line(
                            squiggle_points(egui::pos2(x0, y), x1),
                            Stroke::new(1.0, color),
                        ));
                        let hover_rect =
                            Rect::from_min_max(egui::pos2(x0, y - 5.0), egui::pos2(x1, y + 3.0));
                        ui.interact(
                            hover_rect,
                            egui::Id::new((
                                "kodmoz_diagnostic",
                                index,
                                diagnostic_index,
                                row_index,
                            )),
                            Sense::hover(),
                        )
                        .on_hover_text(format!(
                            "{}: {}",
                            severity_label(diagnostic.severity),
                            diagnostic.message
                        ));
                    }
                }
            }
        }
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        egui::Frame::NONE.fill(theme::BG).show(ui, |ui| {
            self.preview_contents(ui, index, false);
        });
    }

    fn preview_contents(&mut self, ui: &mut egui::Ui, index: usize, split: bool) {
        let tab_id = self.tabs[index].id;
        let is_html = self.tabs[index].language.is_html();
        let is_markdown = self.tabs[index].language.is_markdown();
        if !is_html && !is_markdown {
            return;
        }
        let content = self.tabs[index].content.clone();

        let markdown = if is_html {
            let hash = hash_str(&content);
            let cached = self
                .html_preview_cache
                .get(&tab_id)
                .filter(|(cached_hash, _)| *cached_hash == hash)
                .map(|(_, markdown)| markdown.clone());
            if let Some(markdown) = cached {
                markdown
            } else {
                let markdown = htmd::convert(&content).unwrap_or_else(|_| content.clone());
                self.html_preview_cache
                    .insert(tab_id, (hash, markdown.clone()));
                markdown
            }
        } else {
            content
        };

        egui::Frame::NONE
            .fill(theme::PANEL_BG)
            .inner_margin(Margin::symmetric(10, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                    theme::paint_icon(ui.painter(), icon_rect, theme::Icon::Eye, theme::ACCENT);
                    ui.label(
                        RichText::new(if is_html {
                            "HTML ÖNİZLEME"
                        } else {
                            "MARKDOWN ÖNİZLEME"
                        })
                        .size(9.5)
                        .strong()
                        .color(theme::FAINT),
                    );
                    if is_html {
                        theme::chip(ui, "statik", theme::MUTED, theme::SURFACE);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::icon_only_button(ui, theme::Icon::Edit, "Düzenleme moduna geç")
                            .clicked()
                        {
                            self.tabs[index].view_mode = crate::app::TabViewMode::Edit;
                        }
                    });
                });
            });

        let cache = &mut self.markdown_cache;
        egui::ScrollArea::vertical()
            .id_salt(("kodmoz_preview_scroll", tab_id, split))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 12.0).max(80.0));
                        egui_commonmark::CommonMarkViewer::new().show(ui, cache, &markdown);
                    });
                });
                ui.add_space(20.0);
            });
    }

    fn image_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        let path = self.tabs[index].path.clone();
        let name = self.tabs[index].name.clone();
        let file_size = self.tabs[index].file_size;
        let extension = path
            .as_deref()
            .and_then(Path::extension)
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_string();

        if self.tabs[index].image_texture.is_none() && self.tabs[index].image_error.is_none() {
            match path.as_deref() {
                Some(path) => match load_image_texture(ui.ctx(), path) {
                    Ok((texture, pixels)) => {
                        let tab = &mut self.tabs[index];
                        tab.image_texture = Some(texture);
                        tab.image_pixels = Some(pixels);
                    }
                    Err(error) => {
                        self.tabs[index].image_error = Some(error);
                    }
                },
                None => {
                    self.tabs[index].image_error = Some("Dosya yolu yok".to_string());
                }
            }
        }

        let pixels = self.tabs[index].image_pixels;
        let error = self.tabs[index].image_error.clone();
        let fit = self.tabs[index].image_fit;
        let texture_id = self.tabs[index]
            .image_texture
            .as_ref()
            .map(|texture| texture.id());
        let mut toggle_fit = false;

        egui::Frame::NONE
            .fill(theme::SURFACE)
            .inner_margin(Margin::symmetric(12, 7))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (badge, _) = ui.allocate_exact_size(Vec2::new(24.0, 16.0), Sense::hover());
                    theme::paint_file_badge(ui.painter(), badge, &extension);
                    ui.add_space(2.0);
                    ui.label(RichText::new(&name).size(11.5).strong().color(theme::TEXT));
                    if let Some((width, height)) = pixels {
                        ui.add_space(6.0);
                        let (icon_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(11.0), Sense::hover());
                        theme::paint_icon(
                            ui.painter(),
                            icon_rect,
                            theme::Icon::Image,
                            theme::FAINT,
                        );
                        ui.label(
                            RichText::new(format!("{width} × {height}"))
                                .size(10.0)
                                .color(theme::MUTED),
                        );
                    }
                    if let Some(bytes) = file_size {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format_bytes(bytes))
                                .size(10.0)
                                .color(theme::FAINT),
                        );
                    }
                    ui.add_space(6.0);
                    let (lock_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                    theme::paint_icon(ui.painter(), lock_rect, theme::Icon::Lock, theme::MUTED);
                    theme::chip(ui, "salt okunur", theme::MUTED, theme::SURFACE_HI);

                    ui.add_space((ui.available_width() - 62.0).max(0.0));
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(if fit { "1:1" } else { "Sığdır" })
                                    .size(10.0)
                                    .color(theme::TEXT),
                            )
                            .min_size(Vec2::new(62.0, 20.0)),
                        )
                        .clicked()
                    {
                        toggle_fit = true;
                    }
                });
            });
        ui.painter().line_segment(
            [
                egui::pos2(ui.max_rect().left(), ui.cursor().top()),
                egui::pos2(ui.max_rect().right(), ui.cursor().top()),
            ],
            Stroke::new(1.0, theme::BORDER),
        );

        if toggle_fit {
            self.tabs[index].image_fit = !fit;
        }

        egui::Frame::NONE
            .inner_margin(Margin::symmetric(16, 16))
            .show(ui, |ui| {
                if let Some(error) = &error {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.22).max(40.0));
                        ui.label(
                            RichText::new("Görsel yüklenemedi")
                                .size(15.0)
                                .strong()
                                .color(theme::RED),
                        );
                        ui.add_space(6.0);
                        ui.label(RichText::new(error).size(11.0).color(theme::MUTED));
                    });
                    return;
                }

                let Some(texture_id) = texture_id else {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.22).max(40.0));
                        ui.label(
                            RichText::new("Görsel yükleniyor...")
                                .size(12.0)
                                .color(theme::MUTED),
                        );
                    });
                    return;
                };

                let (width, height) = pixels.unwrap_or((1, 1));
                let available = ui.available_size();
                let scale = if fit {
                    ((available.x - 40.0) / width as f32)
                        .min((available.y - 40.0) / height as f32)
                        .clamp(0.01, 1.0)
                } else {
                    1.0
                };
                let display = Vec2::new(
                    (width as f32 * scale).max(1.0),
                    (height as f32 * scale).max(1.0),
                );

                egui::ScrollArea::both()
                    .id_salt(("kodmoz_image_scroll", self.tabs[index].id))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let spare = ((ui.available_width() - display.x) * 0.5).max(0.0);
                        ui.horizontal(|ui| {
                            ui.add_space(spare);
                            let (rect, _) = ui.allocate_exact_size(display, Sense::hover());
                            let painter =
                                ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                            paint_checkerboard(&painter, rect);
                            painter.image(
                                texture_id,
                                rect,
                                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        });
                    });
            });
    }

    fn binary_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        let name = self.tabs[index].name.clone();
        let file_size = self.tabs[index].file_size;
        let extension = self.tabs[index]
            .path
            .as_deref()
            .and_then(Path::extension)
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_string();

        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.22).max(48.0));
            let (badge, _) = ui.allocate_exact_size(Vec2::new(40.0, 26.0), Sense::hover());
            theme::paint_file_badge(ui.painter(), badge, &extension);
            ui.add_space(16.0);
            ui.label(RichText::new(name).size(16.0).strong().color(theme::TEXT));
            ui.add_space(6.0);
            ui.label(
                RichText::new("İkili dosya metin olarak açılamaz")
                    .size(11.5)
                    .color(theme::MUTED),
            );
            if let Some(bytes) = file_size {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("{} · salt okunur", format_bytes(bytes)))
                        .size(10.0)
                        .color(theme::FAINT),
                );
            }
        });
    }
}

fn load_image_texture(
    ctx: &egui::Context,
    path: &Path,
) -> Result<(egui::TextureHandle, (u32, u32)), String> {
    let image = image::open(path).map_err(|error| format!("{error}"))?;
    let (width, height) = (image.width(), image.height());
    let image = if width.max(height) > 8192 {
        image.thumbnail(8192, 8192)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let texture = ctx.load_texture(
        format!("kodmoz-image:{}", path.display()),
        color_image,
        egui::TextureOptions::LINEAR,
    );
    Ok((texture, (width, height)))
}

fn paint_checkerboard(painter: &egui::Painter, rect: Rect) {
    let visible = rect.intersect(painter.clip_rect());
    if !visible.is_positive() {
        return;
    }
    let dark = Color32::from_rgb(0x18, 0x19, 0x1D);
    let light = Color32::from_rgb(0x20, 0x22, 0x28);
    painter.rect_filled(rect, CornerRadius::ZERO, dark);

    let cell = 12.0;
    let start_x = rect.left() + ((visible.left() - rect.left()) / cell).floor() * cell;
    let start_y = rect.top() + ((visible.top() - rect.top()) / cell).floor() * cell;
    let mut y = start_y;
    let mut row = ((start_y - rect.top()) / cell) as i32;
    while y < visible.bottom() {
        let mut x = start_x;
        let mut column = ((start_x - rect.left()) / cell) as i32;
        while x < visible.right() {
            if (row + column) % 2 == 0 {
                painter.rect_filled(
                    Rect::from_min_size(egui::pos2(x, y), Vec2::splat(cell)),
                    CornerRadius::ZERO,
                    light,
                );
            }
            x += cell;
            column += 1;
        }
        y += cell;
        row += 1;
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn squiggle_points(start: egui::Pos2, end_x: f32) -> Vec<egui::Pos2> {
    let mut points = vec![start];
    let mut x = start.x;
    let mut up = true;
    while x < end_x {
        x = (x + 3.0).min(end_x);
        points.push(egui::pos2(x, start.y + if up { -1.2 } else { 1.2 }));
        up = !up;
    }
    points
}
