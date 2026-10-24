use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use egui::{Align2, CornerRadius, FontId, RichText, Sense, Ui, Vec2};

use crate::theme;

#[derive(Clone)]
struct Entry {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

pub struct Explorer {
    root: PathBuf,
    dirs: HashMap<PathBuf, Vec<Entry>>,
    expanded: HashSet<PathBuf>,
}

impl Explorer {
    pub fn new(root: PathBuf) -> Self {
        let mut explorer = Self {
            root: root.clone(),
            dirs: HashMap::new(),
            expanded: HashSet::new(),
        };
        explorer.expanded.insert(root.clone());
        explorer.ensure_loaded(&root);
        explorer
    }

    pub fn refresh(&mut self) {
        self.dirs.clear();
        let root = self.root.clone();
        self.ensure_loaded(&root);
    }

    /// Expands every ancestor of `path` so the file becomes visible in the tree.
    pub fn reveal(&mut self, path: &Path) {
        let mut current = path.parent();
        while let Some(dir) = current {
            if !dir.starts_with(&self.root) {
                break;
            }
            self.expanded.insert(dir.to_path_buf());
            if dir == self.root {
                break;
            }
            current = dir.parent();
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, active: Option<&Path>) -> Option<PathBuf> {
        let root = self.root.clone();
        let root_name = root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| root.display().to_string());
        let root_expanded = self.expanded.contains(&root);
        let mut clicked = None;

        egui::ScrollArea::vertical()
            .id_salt("kodmoz_explorer_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;

                let response = draw_row(ui, 0, &root_name, true, root_expanded, false, &root);
                if response.clicked() {
                    if root_expanded {
                        self.expanded.remove(&root);
                    } else {
                        self.expanded.insert(root.clone());
                    }
                }

                if self.expanded.contains(&root) {
                    let empty = self
                        .dirs
                        .get(&root)
                        .map(|entries| entries.is_empty())
                        .unwrap_or(false);
                    if empty {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Klasör boş").size(10.0).color(theme::FAINT));
                    } else if let Some(path) = self.render_dir(ui, &root, 1, active) {
                        clicked = Some(path);
                    }
                }
            });

        clicked
    }

    fn render_dir(
        &mut self,
        ui: &mut Ui,
        dir: &Path,
        depth: usize,
        active: Option<&Path>,
    ) -> Option<PathBuf> {
        let entries = self.children(dir);
        let mut clicked = None;

        for entry in entries {
            let expanded = entry.is_dir && self.expanded.contains(&entry.path);
            let selected = !entry.is_dir && active == Some(entry.path.as_path());

            let response = draw_row(
                ui,
                depth,
                &entry.name,
                entry.is_dir,
                expanded,
                selected,
                &entry.path,
            );

            if response.clicked() {
                if entry.is_dir {
                    if expanded {
                        self.expanded.remove(&entry.path);
                    } else {
                        self.expanded.insert(entry.path.clone());
                    }
                } else {
                    clicked = Some(entry.path.clone());
                }
            }

            if entry.is_dir && self.expanded.contains(&entry.path) {
                if let Some(path) = self.render_dir(ui, &entry.path, depth + 1, active) {
                    clicked = Some(path);
                }
            }
        }

        clicked
    }

    fn children(&mut self, dir: &Path) -> Vec<Entry> {
        self.ensure_loaded(dir);
        self.dirs.get(dir).cloned().unwrap_or_default()
    }

    fn ensure_loaded(&mut self, dir: &Path) {
        if self.dirs.contains_key(dir) {
            return;
        }
        let mut entries = Vec::new();
        if let Ok(read_dir) = std::fs::read_dir(dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
                if is_dir && matches!(name.as_str(), "target" | "node_modules") {
                    continue;
                }
                entries.push(Entry { path, name, is_dir });
            }
        }
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.dirs.insert(dir.to_path_buf(), entries);
    }
}

fn draw_row(
    ui: &mut Ui,
    depth: usize,
    name: &str,
    is_dir: bool,
    expanded: bool,
    selected: bool,
    full_path: &Path,
) -> egui::Response {
    let row_height = 21.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height), Sense::click());
    let painter = ui.painter().with_clip_rect(rect);

    if selected {
        painter.rect_filled(rect, CornerRadius::same(4), theme::ACCENT_SOFT);
        painter.rect_filled(
            egui::Rect::from_min_size(rect.left_top(), Vec2::new(2.0, rect.height())),
            CornerRadius::ZERO,
            theme::ACCENT,
        );
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(4), theme::SURFACE);
    }

    let indent = 6.0 + depth as f32 * 12.0;
    let center_y = rect.center().y;
    let text_x;

    if is_dir {
        theme::paint_chevron(
            &painter,
            egui::pos2(rect.left() + indent + 3.0, center_y),
            expanded,
            if response.hovered() || selected {
                theme::MUTED
            } else {
                theme::FAINT
            },
        );
        let folder_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + indent + 10.0, center_y - 6.0),
            Vec2::new(16.0, 13.0),
        );
        theme::paint_folder(&painter, folder_rect);
        text_x = rect.left() + indent + 31.0;
    } else {
        let extension = full_path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("");
        let badge = egui::Rect::from_min_size(
            egui::pos2(rect.left() + indent + 6.0, center_y - 7.0),
            Vec2::new(22.0, 14.0),
        );
        theme::paint_file_badge(&painter, badge, extension);
        text_x = rect.left() + indent + 34.0;
    }

    let color = if selected {
        theme::TEXT
    } else if is_dir {
        theme::TEXT
    } else {
        theme::MUTED
    };
    painter.text(
        egui::pos2(text_x, center_y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(11.5),
        color,
    );

    response.on_hover_text(full_path.display().to_string())
}
