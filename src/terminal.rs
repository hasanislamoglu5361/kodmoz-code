use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use egui::{Align2, Color32, FontId, Id, Margin, RichText, Sense, Stroke, Vec2};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::app::KodmozCode;
use crate::theme;

const SCROLLBACK_LINES: usize = 5000;

pub struct TerminalSession {
    parser: Arc<Mutex<vt100::Parser>>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    rows: u16,
    cols: u16,
    cwd: PathBuf,
    shell: String,
    exited: bool,
}

impl TerminalSession {
    pub fn spawn(cwd: &Path, rows: u16, cols: u16) -> Result<Self, String> {
        let rows = rows.max(2);
        let cols = cols.max(10);
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("PTY açılamadı: {error}"))?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let mut command = CommandBuilder::new(&shell);
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        if let Some(path) = crate::lsp::augmented_path() {
            command.env("PATH", path);
        }

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("Kabuk başlatılamadı: {error}"))?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("Terminal okuyucusu açılamadı: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("Terminal yazıcısı açılamadı: {error}"))?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(
            NonZeroU16::new(rows).unwrap(),
            NonZeroU16::new(cols).unwrap(),
            SCROLLBACK_LINES,
        )));
        let parser_clone = Arc::clone(&parser);
        std::thread::Builder::new()
            .name("kodmoz-terminal-reader".to_string())
            .spawn(move || {
                let mut buffer = [0u8; 8192];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(read) => {
                            if let Ok(mut parser) = parser_clone.lock() {
                                parser.process(&buffer[..read]);
                            } else {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|error| format!("Terminal thread'i başlatılamadı: {error}"))?;

        Ok(Self {
            parser,
            writer,
            master: pair.master,
            child,
            rows,
            cols,
            cwd: cwd.to_path_buf(),
            shell,
            exited: false,
        })
    }

    pub fn lock_parser(&self) -> MutexGuard<'_, vt100::Parser> {
        match self.parser.lock() {
            Ok(parser) => parser,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows = rows.max(2);
        let cols = cols.max(10);
        if rows == self.rows && cols == self.cols {
            return;
        }
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        if let Ok(mut parser) = self.parser.lock() {
            parser.set_size(
                NonZeroU16::new(rows).unwrap(),
                NonZeroU16::new(cols).unwrap(),
            );
        }
        self.rows = rows;
        self.cols = cols;
    }

    pub fn is_exited(&mut self) -> bool {
        if self.exited {
            return true;
        }
        match self.child.try_wait() {
            Ok(Some(_)) => {
                self.exited = true;
                true
            }
            Ok(None) => false,
            Err(_) => {
                self.exited = true;
                true
            }
        }
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn shell(&self) -> &str {
        &self.shell
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

impl KodmozCode {
    pub(crate) fn ensure_terminal(&mut self, rows: u16, cols: u16) {
        if self.terminal.is_some() || self.terminal_error.is_some() {
            return;
        }
        let cwd = self
            .root
            .clone()
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        match TerminalSession::spawn(&cwd, rows, cols) {
            Ok(session) => {
                self.terminal = Some(session);
                self.terminal_focus_requested = true;
            }
            Err(error) => {
                self.terminal_error = Some(error);
            }
        }
    }

    pub(crate) fn terminal_panel(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut restart = false;
        let mut clear = false;

        egui::TopBottomPanel::bottom("kodmoz_terminal_panel")
            .resizable(true)
            .default_height(230.0)
            .min_height(110.0)
            .frame(egui::Frame::NONE.fill(theme::BG))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme::SURFACE)
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (icon_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                            theme::paint_icon(
                                ui.painter(),
                                icon_rect,
                                theme::Icon::Terminal,
                                theme::ACCENT,
                            );
                            ui.label(
                                RichText::new("TERMİNAL")
                                    .size(9.5)
                                    .strong()
                                    .color(theme::FAINT),
                            );
                            if let Some(terminal) = &mut self.terminal {
                                ui.label(
                                    RichText::new(terminal.cwd().display().to_string())
                                        .size(9.5)
                                        .color(theme::MUTED),
                                )
                                .on_hover_text(format!(
                                    "{} · {}",
                                    terminal.shell(),
                                    terminal.cwd().display()
                                ));
                                if terminal.is_exited() {
                                    ui.label(
                                        RichText::new("süreç sonlandı")
                                            .size(9.0)
                                            .color(theme::YELLOW),
                                    );
                                }
                            }
                            let reserved = 3.0 * 24.0 + 8.0;
                            ui.add_space((ui.available_width() - reserved).max(0.0));
                            if theme::icon_only_button(ui, theme::Icon::Trash, "Temizle").clicked()
                            {
                                clear = true;
                            }
                            if theme::icon_only_button(
                                ui,
                                theme::Icon::Refresh,
                                "Terminali yeniden başlat",
                            )
                            .clicked()
                            {
                                restart = true;
                            }
                            if theme::close_button(ui).clicked() {
                                close = true;
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

                let body_rect = ui.available_rect_before_wrap();
                let terminal_font_size = (self.font_size * 0.86).clamp(9.0, 24.0);
                let font = FontId::monospace(terminal_font_size);
                let cell_width = ui.fonts(|fonts| {
                    fonts
                        .layout_no_wrap("M".to_owned(), font.clone(), Color32::WHITE)
                        .size()
                        .x
                });
                let row_height = ui.fonts(|fonts| fonts.row_height(&font));
                let cols = ((body_rect.width() - 16.0) / cell_width).floor().max(10.0) as u16;
                let rows = ((body_rect.height() - 12.0) / row_height).floor().max(2.0) as u16;

                self.ensure_terminal(rows, cols);

                if let Some(error) = &self.terminal_error {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.25).max(20.0));
                        let (icon_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
                        theme::paint_icon(
                            ui.painter(),
                            icon_rect,
                            theme::Icon::Problems,
                            theme::RED,
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Terminal başlatılamadı")
                                .size(13.0)
                                .strong()
                                .color(theme::TEXT),
                        );
                        ui.add_space(4.0);
                        ui.label(RichText::new(error).size(10.5).color(theme::MUTED));
                        ui.add_space(8.0);
                        if ui.button("Tekrar dene").clicked() {
                            restart = true;
                        }
                    });
                    return;
                }

                let Some(terminal) = &mut self.terminal else {
                    return;
                };
                terminal.resize(rows, cols);

                let body = body_rect.shrink2(Vec2::new(8.0, 6.0));
                ui.allocate_rect(body, Sense::hover());
                let response = ui
                    .interact(body, Id::new("kodmoz_terminal_body"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::Text);
                if response.clicked() {
                    response.request_focus();
                }
                if self.terminal_focus_requested {
                    response.request_focus();
                    self.terminal_focus_requested = false;
                }
                let focused = response.has_focus();

                if focused {
                    let events = ui.input(|input| input.events.clone());
                    let mut data: Vec<u8> = Vec::new();
                    for event in events {
                        match event {
                            egui::Event::Text(text) => data.extend_from_slice(text.as_bytes()),
                            egui::Event::Paste(text) => data.extend_from_slice(text.as_bytes()),
                            egui::Event::Key {
                                key,
                                pressed: true,
                                modifiers,
                                ..
                            } => {
                                if modifiers.command {
                                    continue;
                                }
                                if modifiers.ctrl {
                                    if let Some(code) = control_code(key) {
                                        data.push(code);
                                    }
                                    continue;
                                }
                                match key {
                                    egui::Key::Enter => data.push(b'\r'),
                                    egui::Key::Backspace => data.push(0x7f),
                                    egui::Key::Tab => {
                                        if modifiers.shift {
                                            data.extend_from_slice(b"\x1b[Z");
                                        } else {
                                            data.push(b'\t');
                                        }
                                    }
                                    egui::Key::Escape => data.push(0x1b),
                                    egui::Key::ArrowUp => data.extend_from_slice(b"\x1b[A"),
                                    egui::Key::ArrowDown => data.extend_from_slice(b"\x1b[B"),
                                    egui::Key::ArrowRight => data.extend_from_slice(b"\x1b[C"),
                                    egui::Key::ArrowLeft => data.extend_from_slice(b"\x1b[D"),
                                    egui::Key::Home => data.extend_from_slice(b"\x1b[H"),
                                    egui::Key::End => data.extend_from_slice(b"\x1b[F"),
                                    egui::Key::PageUp => data.extend_from_slice(b"\x1b[5~"),
                                    egui::Key::PageDown => data.extend_from_slice(b"\x1b[6~"),
                                    egui::Key::Delete => data.extend_from_slice(b"\x1b[3~"),
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                    if !data.is_empty() {
                        terminal.write(&data);
                        let mut parser = terminal.lock_parser();
                        parser.screen_mut().set_scrollback(0);
                    }
                }

                if response.hovered() {
                    let scroll = ui.input(|input| input.smooth_scroll_delta.y);
                    if scroll.abs() > 1.0 {
                        let mut parser = terminal.lock_parser();
                        let screen = parser.screen_mut();
                        let current = screen.scrollback() as i32;
                        let max = screen.scrollback_len() as i32;
                        let target = (current + (scroll / row_height).round() as i32).clamp(0, max);
                        screen.set_scrollback(target as usize);
                    }
                }

                if clear {
                    terminal.write(b"\x0c");
                }

                let painter = ui.painter().with_clip_rect(body);
                painter.rect_filled(body, egui::CornerRadius::ZERO, theme::BG);

                let parser = terminal.lock_parser();
                let screen = parser.screen();
                let scrolled_back = screen.scrollback() > 0;

                for row in 0..rows {
                    let mut run_start = 0u16;
                    let mut run_cells = 0u16;
                    let mut run_text = String::new();
                    let mut run_style: Option<RunStyle> = None;

                    for col in 0..=cols {
                        let style = if col < cols {
                            screen.cell(row, col).map(|cell| cell_style(cell))
                        } else {
                            None
                        };
                        let style = style.unwrap_or(RunStyle::default());
                        let at_end = col == cols;

                        if run_style != Some(style) {
                            if let Some(previous) = run_style {
                                paint_run(
                                    &painter, body, row, run_start, run_cells, &run_text, previous,
                                    &font, cell_width, row_height,
                                );
                            }
                            run_start = col;
                            run_cells = 0;
                            run_text.clear();
                            run_style = Some(style);
                        }

                        if !at_end {
                            run_cells += 1;
                            if let Some(cell) = screen.cell(row, col) {
                                run_text.push_str(cell.contents());
                            }
                        }
                    }

                    if let Some(style) = run_style {
                        paint_run(
                            &painter, body, row, run_start, run_cells, &run_text, style, &font,
                            cell_width, row_height,
                        );
                    }
                }

                if focused && !scrolled_back && !screen.hide_cursor() {
                    let (cursor_row, cursor_col) = screen.cursor_position();
                    if cursor_row < rows && cursor_col < cols {
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(
                                body.left() + cursor_col as f32 * cell_width,
                                body.top() + cursor_row as f32 * row_height,
                            ),
                            Vec2::new(cell_width, row_height),
                        );
                        painter.rect_filled(
                            rect,
                            egui::CornerRadius::ZERO,
                            Color32::from_rgba_unmultiplied(232, 167, 93, 110),
                        );
                    }
                }

                if !focused {
                    painter.rect_stroke(
                        body,
                        egui::CornerRadius::ZERO,
                        Stroke::new(1.0, theme::BORDER),
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        body.right_bottom() + Vec2::new(-10.0, -8.0),
                        Align2::RIGHT_BOTTOM,
                        "yazmak için tıkla",
                        FontId::proportional(9.5),
                        theme::FAINT,
                    );
                }
            });

        if close {
            self.terminal_visible = false;
        }
        if restart {
            self.terminal = None;
            self.terminal_error = None;
            self.terminal_focus_requested = true;
        }
        if self.terminal_visible {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct RunStyle {
    fg: Color32,
    bg: Color32,
    underline: bool,
}

impl Default for RunStyle {
    fn default() -> Self {
        Self {
            fg: theme::TEXT,
            bg: theme::BG,
            underline: false,
        }
    }
}

fn cell_style(cell: &vt100::Cell) -> RunStyle {
    let mut fg = ansi_color(cell.fgcolor(), true);
    let mut bg = ansi_color(cell.bgcolor(), false);
    if cell.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cell.bold() && fg == theme::TEXT {
        fg = Color32::WHITE;
    }
    if cell.dim() {
        fg = fg.gamma_multiply(0.6);
    }
    RunStyle {
        fg,
        bg,
        underline: cell.underline(),
    }
}

fn ansi_color(color: vt100::Color, foreground: bool) -> Color32 {
    let palette: [Color32; 16] = [
        Color32::from_rgb(0x1B, 0x1D, 0x22),
        Color32::from_rgb(0xE0, 0x6C, 0x75),
        Color32::from_rgb(0x98, 0xC3, 0x79),
        Color32::from_rgb(0xE5, 0xC0, 0x7B),
        Color32::from_rgb(0x61, 0xAF, 0xEF),
        Color32::from_rgb(0xC6, 0x78, 0xDD),
        Color32::from_rgb(0x56, 0xB6, 0xC2),
        Color32::from_rgb(0xD7, 0xD9, 0xE0),
        Color32::from_rgb(0x5C, 0x61, 0x6C),
        Color32::from_rgb(0xFF, 0x7B, 0x86),
        Color32::from_rgb(0xB5, 0xE8, 0x90),
        Color32::from_rgb(0xFF, 0xD6, 0x8A),
        Color32::from_rgb(0x7C, 0xC4, 0xFF),
        Color32::from_rgb(0xE2, 0x9B, 0xFF),
        Color32::from_rgb(0x7F, 0xE0, 0xE8),
        Color32::WHITE,
    ];
    match color {
        vt100::Color::Default => {
            if foreground {
                theme::TEXT
            } else {
                theme::BG
            }
        }
        vt100::Color::Idx(index) => {
            if (index as usize) < 16 {
                palette[index as usize]
            } else if index < 232 {
                let index = index - 16;
                let r = (index / 36) % 6;
                let g = (index / 6) % 6;
                let b = index % 6;
                let component = |value: u8| -> u8 {
                    match value {
                        0 => 0,
                        1 => 95,
                        2 => 135,
                        3 => 175,
                        4 => 215,
                        _ => 255,
                    }
                };
                Color32::from_rgb(component(r), component(g), component(b))
            } else {
                let value = 8 + (index - 232) * 10;
                Color32::from_gray(value)
            }
        }
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_run(
    painter: &egui::Painter,
    body: egui::Rect,
    row: u16,
    run_start: u16,
    run_cells: u16,
    text: &str,
    style: RunStyle,
    font: &FontId,
    cell_width: f32,
    row_height: f32,
) {
    if run_cells == 0 {
        return;
    }
    let rect = egui::Rect::from_min_size(
        egui::pos2(
            body.left() + run_start as f32 * cell_width,
            body.top() + row as f32 * row_height,
        ),
        Vec2::new(run_cells as f32 * cell_width, row_height),
    );
    if style.bg != theme::BG {
        painter.rect_filled(rect, egui::CornerRadius::ZERO, style.bg);
    }
    if text.trim().is_empty() {
        return;
    }
    painter.text(
        rect.left_top(),
        Align2::LEFT_TOP,
        text,
        font.clone(),
        style.fg,
    );
    if style.underline {
        painter.line_segment(
            [
                egui::pos2(rect.left(), rect.bottom() - 1.5),
                egui::pos2(rect.right(), rect.bottom() - 1.5),
            ],
            Stroke::new(1.0, style.fg),
        );
    }
}

fn control_code(key: egui::Key) -> Option<u8> {
    let code = match key {
        egui::Key::A => 1,
        egui::Key::B => 2,
        egui::Key::C => 3,
        egui::Key::D => 4,
        egui::Key::E => 5,
        egui::Key::F => 6,
        egui::Key::G => 7,
        egui::Key::H => 8,
        egui::Key::I => 9,
        egui::Key::J => 10,
        egui::Key::K => 11,
        egui::Key::L => 12,
        egui::Key::M => 13,
        egui::Key::N => 14,
        egui::Key::O => 15,
        egui::Key::P => 16,
        egui::Key::Q => 17,
        egui::Key::R => 18,
        egui::Key::S => 19,
        egui::Key::T => 20,
        egui::Key::U => 21,
        egui::Key::V => 22,
        egui::Key::W => 23,
        egui::Key::X => 24,
        egui::Key::Y => 25,
        egui::Key::Z => 26,
        egui::Key::Space => 0,
        _ => return None,
    };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn pty_echoes_a_command() {
        let mut session =
            TerminalSession::spawn(&std::env::temp_dir(), 24, 80).expect("terminal spawn");
        session.write(b"printf 'KODMOZ_TERMINAL_OK\\n'\n");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            {
                let parser = session.lock_parser();
                if parser.screen().contents().contains("KODMOZ_TERMINAL_OK") {
                    break;
                }
            }
            if Instant::now() > deadline {
                let contents = session.lock_parser().screen().contents();
                panic!("terminal output not received: {contents:?}");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
