use std::path::{Path, PathBuf};

use egui::{Align, Align2, Context, Id, Key, Modifiers, RichText, TextEdit};

use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandId {
    OpenFolder,
    NewFile,
    Save,
    SaveAs,
    CloseTab,
    QuickOpen,
    Palette,
    ToggleTerminal,
    ToggleSidebar,
    ToggleDiagnostics,
    ZoomIn,
    ZoomOut,
    Quit,
}

pub const COMMANDS: &[(&str, &str, CommandId)] = &[
    ("Klasör Aç...", "Cmd+O", CommandId::OpenFolder),
    ("Yeni Dosya", "Cmd+N", CommandId::NewFile),
    ("Kaydet", "Cmd+S", CommandId::Save),
    ("Farklı Kaydet...", "Cmd+Shift+S", CommandId::SaveAs),
    ("Sekmeyi Kapat", "Cmd+W", CommandId::CloseTab),
    ("Dosyaya Git...", "Cmd+P", CommandId::QuickOpen),
    ("Komut Paleti", "Cmd+Shift+P", CommandId::Palette),
    ("Terminali Aç/Kapat", "Cmd+`", CommandId::ToggleTerminal),
    ("Kenar Çubuğunu Aç/Kapat", "Cmd+B", CommandId::ToggleSidebar),
    (
        "Sorunlar Panelini Aç/Kapat",
        "Cmd+Shift+M",
        CommandId::ToggleDiagnostics,
    ),
    ("Yakınlaştır", "Cmd+=", CommandId::ZoomIn),
    ("Uzaklaştır", "Cmd+-", CommandId::ZoomOut),
    ("Çıkış", "Cmd+Q", CommandId::Quit),
];

pub enum QuickOpenResult {
    Keep,
    Close,
    Open(PathBuf),
}

pub struct QuickOpen {
    root: PathBuf,
    files: Vec<PathBuf>,
    query: String,
    results: Vec<PathBuf>,
    selected: usize,
    needs_focus: bool,
    needs_scroll: bool,
}

impl QuickOpen {
    pub fn new(root: PathBuf) -> Self {
        let files = collect_files(&root);
        let mut dialog = Self {
            root,
            files,
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            needs_focus: true,
            needs_scroll: false,
        };
        dialog.refilter();
        dialog
    }

    fn refilter(&mut self) {
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            self.results = self.files.iter().take(80).cloned().collect();
        } else {
            let mut scored: Vec<(f64, &PathBuf)> = self
                .files
                .iter()
                .filter_map(|path| {
                    let relative = path.strip_prefix(&self.root).unwrap_or(path);
                    let relative = relative.to_string_lossy().replace('\\', "/");
                    fuzzy_score(&query, &relative).map(|score| (score, path))
                })
                .collect();
            scored.sort_by(|a, b| {
                b.0.partial_cmp(&a.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.1.as_os_str().len().cmp(&b.1.as_os_str().len()))
            });
            self.results = scored
                .into_iter()
                .take(80)
                .map(|(_, path)| path.clone())
                .collect();
        }
        self.selected = 0;
        self.needs_scroll = false;
    }

    pub fn show(&mut self, ctx: &Context) -> QuickOpenResult {
        let mut result = QuickOpenResult::Keep;

        let escape = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
        if escape {
            return QuickOpenResult::Close;
        }
        let down = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowDown));
        let up = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowUp));
        if down && !self.results.is_empty() {
            self.selected = (self.selected + 1) % self.results.len();
            self.needs_scroll = true;
        }
        if up && !self.results.is_empty() {
            self.selected = (self.selected + self.results.len() - 1) % self.results.len();
            self.needs_scroll = true;
        }
        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
            if let Some(path) = self.results.get(self.selected) {
                return QuickOpenResult::Open(path.clone());
            }
        }

        egui::Area::new(Id::new("kodmoz_quick_open"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 64.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(720.0);
                    ui.label(
                        RichText::new("Dosyaya Git")
                            .strong()
                            .size(12.0)
                            .color(theme::TEXT),
                    );
                    let response = ui.add(
                        TextEdit::singleline(&mut self.query)
                            .hint_text("Dosya adı yaz...")
                            .desired_width(f32::INFINITY),
                    );
                    if self.needs_focus {
                        response.request_focus();
                        self.needs_focus = false;
                    }
                    if response.changed() {
                        self.refilter();
                    }
                    ui.add_space(4.0);
                    ui.separator();

                    let mut selected_response = None;
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if self.results.is_empty() {
                                ui.label(RichText::new("Sonuç yok").size(11.0).color(theme::FAINT));
                            }
                            for (index, path) in self.results.iter().enumerate() {
                                let relative = path.strip_prefix(&self.root).unwrap_or(path);
                                let relative = relative.to_string_lossy().replace('\\', "/");
                                let selected = index == self.selected;
                                let response = ui
                                    .selectable_label(selected, RichText::new(relative).size(11.5));
                                if selected {
                                    selected_response = Some(response.clone());
                                }
                                if response.clicked() {
                                    result = QuickOpenResult::Open(path.clone());
                                }
                            }
                        });
                    if self.needs_scroll {
                        if let Some(response) = selected_response {
                            response.scroll_to_me(Some(Align::Center));
                        }
                        self.needs_scroll = false;
                    }
                });
            });

        result
    }
}

pub enum PaletteResult {
    Keep,
    Close,
    Run(CommandId),
}

pub struct Palette {
    query: String,
    selected: usize,
    needs_focus: bool,
    needs_scroll: bool,
}

impl Palette {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            selected: 0,
            needs_focus: true,
            needs_scroll: false,
        }
    }

    fn filtered(&self) -> Vec<(usize, &'static str, &'static str)> {
        let query = self.query.trim().to_lowercase();
        COMMANDS
            .iter()
            .enumerate()
            .filter(|(_, (label, _, _))| query.is_empty() || label.to_lowercase().contains(&query))
            .map(|(index, (label, shortcut, _))| (index, *label, *shortcut))
            .collect()
    }

    pub fn show(&mut self, ctx: &Context) -> PaletteResult {
        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
            return PaletteResult::Close;
        }

        let filtered = self.filtered();
        if !filtered.is_empty() && self.selected >= filtered.len() {
            self.selected = filtered.len() - 1;
        }
        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowDown))
            && !filtered.is_empty()
        {
            self.selected = (self.selected + 1) % filtered.len();
            self.needs_scroll = true;
        }
        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowUp))
            && !filtered.is_empty()
        {
            self.selected = (self.selected + filtered.len() - 1) % filtered.len();
            self.needs_scroll = true;
        }
        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
            if let Some((command_index, _, _)) = filtered.get(self.selected) {
                return PaletteResult::Run(COMMANDS[*command_index].2);
            }
        }

        let mut result = PaletteResult::Keep;
        egui::Area::new(Id::new("kodmoz_palette"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 64.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(720.0);
                    ui.label(
                        RichText::new("Komut Paleti")
                            .strong()
                            .size(12.0)
                            .color(theme::TEXT),
                    );
                    let response = ui.add(
                        TextEdit::singleline(&mut self.query)
                            .hint_text("Komut ara...")
                            .desired_width(f32::INFINITY),
                    );
                    if self.needs_focus {
                        response.request_focus();
                        self.needs_focus = false;
                    }
                    if response.changed() {
                        self.selected = 0;
                    }
                    ui.add_space(4.0);
                    ui.separator();

                    let filtered = self.filtered();
                    let mut selected_response = None;
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if filtered.is_empty() {
                                ui.label(
                                    RichText::new("Komut bulunamadı")
                                        .size(11.0)
                                        .color(theme::FAINT),
                                );
                            }
                            for (row, (command_index, label, shortcut)) in
                                filtered.iter().enumerate()
                            {
                                let selected = row == self.selected;
                                let response = ui.selectable_label(
                                    selected,
                                    RichText::new(format!("{label:<36} {shortcut}")).size(11.5),
                                );
                                if selected {
                                    selected_response = Some(response.clone());
                                }
                                if response.clicked() {
                                    result = PaletteResult::Run(COMMANDS[*command_index].2);
                                }
                            }
                        });
                    if self.needs_scroll {
                        if let Some(response) = selected_response {
                            response.scroll_to_me(Some(Align::Center));
                        }
                        self.needs_scroll = false;
                    }
                });
            });

        result
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new()
    }
}

fn collect_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .build();
    for entry in walker.flatten() {
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let path = entry.into_path();
        let is_skipped = path.components().any(|component| {
            matches!(
                component.as_os_str().to_str(),
                Some("target") | Some(".git") | Some("node_modules")
            )
        });
        if is_skipped {
            continue;
        }
        files.push(path);
        if files.len() >= 30_000 {
            break;
        }
    }
    files
}

fn fuzzy_score(query: &str, candidate: &str) -> Option<f64> {
    let query: Vec<char> = query
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|ch| !ch.is_whitespace())
        .collect();
    if query.is_empty() {
        return Some(0.0);
    }

    let chars: Vec<char> = candidate.chars().flat_map(char::to_lowercase).collect();
    let mut positions = Vec::with_capacity(query.len());
    let mut cursor = 0usize;
    for &needle in &query {
        let mut found = None;
        while cursor < chars.len() {
            if chars[cursor] == needle {
                found = Some(cursor);
                break;
            }
            cursor += 1;
        }
        let index = found?;
        positions.push(index);
        cursor = index + 1;
    }

    let mut score = 0.0;
    for (order, &index) in positions.iter().enumerate() {
        score += 1.0;
        if order > 0 && positions[order - 1] + 1 == index {
            score += 2.0;
        }
        let previous = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
        if index == 0 || matches!(previous, Some('/') | Some('_') | Some('-') | Some('.')) {
            score += 3.0;
        }
    }

    if let Some(last_slash) = chars.iter().rposition(|&ch| ch == '/') {
        if positions.last().copied().unwrap_or(0) > last_slash {
            score += 4.0;
        }
    }

    score -= chars.len() as f64 * 0.01;
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_subsequences() {
        assert!(fuzzy_score("ap", "src/app.rs").is_some());
        assert!(fuzzy_score("srs", "src/app.rs").is_some());
        assert!(fuzzy_score("zzz", "src/app.rs").is_none());
        assert_eq!(fuzzy_score("", "anything"), Some(0.0));
    }

    #[test]
    fn filename_matches_score_higher() {
        let filename = fuzzy_score("app", "src/app.rs").unwrap();
        let directory = fuzzy_score("app", "application/src/lib.rs").unwrap();
        assert!(filename > directory);
    }

    #[test]
    fn command_list_has_expected_entries() {
        assert!(COMMANDS.iter().any(|(_, _, id)| *id == CommandId::Save));
        assert!(COMMANDS.iter().any(|(_, _, id)| *id == CommandId::Palette));
    }
}
