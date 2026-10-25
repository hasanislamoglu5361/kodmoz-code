use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{
    Align, Align2, Color32, FontId, Key, KeyboardShortcut, Layout, Margin, Modifiers, RichText,
    Sense, Stroke, TextStyle, Ui, Vec2,
};
use serde_json::{json, Value};

use crate::dialogs::{CommandId, Palette, PaletteResult, QuickOpen, QuickOpenResult};
use crate::explorer::Explorer;
use crate::highlight::{Highlighter, LanguageId};
use crate::lsp::{
    find_rust_analyzer, path_to_uri, uri_to_path, DiagnosticItem, LspClient, LspEvent,
};
use crate::terminal::TerminalSession;
use crate::theme;

const DEFAULT_FONT_SIZE: f32 = 14.0;
const MIN_FONT_SIZE: f32 = 9.0;
const MAX_FONT_SIZE: f32 = 28.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum TabViewMode {
    #[default]
    Edit,
    Split,
    Preview,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PaneLayout {
    Single,
    TwoColumns,
    TwoRows,
    Three,
    Four,
}

impl PaneLayout {
    fn slots(self) -> usize {
        match self {
            Self::Single => 1,
            Self::TwoColumns | Self::TwoRows => 2,
            Self::Three => 3,
            Self::Four => 4,
        }
    }

    fn tooltip(self) -> &'static str {
        match self {
            Self::Single => "Tek bölme",
            Self::TwoColumns => "İki sütun",
            Self::TwoRows => "İki satır",
            Self::Three => "Üç bölme",
            Self::Four => "Dört bölme (2x2)",
        }
    }

    fn from_env(value: &str) -> Option<Self> {
        match value {
            "1" | "single" => Some(Self::Single),
            "2" | "2c" | "cols" => Some(Self::TwoColumns),
            "2r" | "rows" => Some(Self::TwoRows),
            "3" | "three" => Some(Self::Three),
            "4" | "four" | "grid" => Some(Self::Four),
            _ => None,
        }
    }
}

pub(crate) struct Tab {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub name: String,
    pub content: String,
    pub dirty: bool,
    pub saved_hash: u64,
    pub language: LanguageId,
    pub view_mode: TabViewMode,
    pub version: i64,
    pub lsp_opened: bool,
    pub last_sent_hash: u64,
    pub edit_time: Option<Instant>,
    pub pending_cursor: Option<usize>,
    pub pending_scroll: Option<f32>,
    pub cursor_char: usize,
    pub line_ending: &'static str,
    pub file_size: Option<u64>,
    pub image_texture: Option<egui::TextureHandle>,
    pub image_pixels: Option<(u32, u32)>,
    pub image_error: Option<String>,
    pub image_fit: bool,
}

impl Tab {
    fn serialized_content(&self) -> String {
        if self.line_ending == "\r\n" {
            self.content.replace('\n', "\r\n")
        } else {
            self.content.clone()
        }
    }

    fn is_read_only(&self) -> bool {
        self.language.is_read_only()
    }
}

pub struct KodmozCode {
    pub(crate) root: Option<PathBuf>,
    pub(crate) explorer: Option<Explorer>,
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active: Option<usize>,
    pub(crate) next_tab_id: u64,
    pub(crate) highlighter: Highlighter,
    pub(crate) font_size: f32,
    pub(crate) sidebar_visible: bool,
    pub(crate) diagnostics_visible: bool,
    pub(crate) lsp: Option<LspClient>,
    pub(crate) lsp_ready: bool,
    pub(crate) lsp_init_id: Option<i64>,
    pub(crate) lsp_status: String,
    pub(crate) lsp_tried: bool,
    pub(crate) lsp_started_at: Option<Instant>,
    pub(crate) diagnostics: HashMap<PathBuf, Vec<DiagnosticItem>>,
    pub(crate) quick_open: Option<QuickOpen>,
    pub(crate) palette: Option<Palette>,
    pub(crate) confirm_close: Option<usize>,
    pub(crate) status_msg: Option<(String, Instant)>,
    pub(crate) git_branch: Option<String>,
    pub(crate) terminal: Option<TerminalSession>,
    pub(crate) terminal_visible: bool,
    pub(crate) terminal_error: Option<String>,
    pub(crate) terminal_focus_requested: bool,
    pub(crate) logo: Option<egui::TextureHandle>,
    pub(crate) pane_layout: PaneLayout,
    pub(crate) panes: Vec<Option<usize>>,
    pub(crate) focused_pane: usize,
    pub(crate) markdown_cache: egui_commonmark::CommonMarkCache,
    pub(crate) html_preview_cache: HashMap<u64, (u64, String)>,
    pub(crate) lens_texture: Option<egui::TextureHandle>,
    pub(crate) lens_capture_pending: bool,
    pub(crate) lens_alt_was_down: bool,
    pending_command: Option<CommandId>,
    last_title: String,
}

impl KodmozCode {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        root: Option<PathBuf>,
        files: Vec<PathBuf>,
    ) -> Self {
        configure_style(&cc.egui_ctx);
        let highlighter = Highlighter::new();
        let highlight_error = highlighter
            .query_error()
            .map(|error| format!("Sözdizimi vurgulama devre dışı: {error}"));

        let mut app = Self {
            root: None,
            explorer: None,
            tabs: Vec::new(),
            active: None,
            next_tab_id: 1,
            highlighter,
            font_size: DEFAULT_FONT_SIZE,
            sidebar_visible: true,
            diagnostics_visible: std::env::var_os("KODMOZ_PROBLEMS").is_some(),
            lsp: None,
            lsp_ready: false,
            lsp_init_id: None,
            lsp_status: "LSP kapalı".to_string(),
            lsp_tried: false,
            lsp_started_at: None,
            diagnostics: HashMap::new(),
            quick_open: None,
            palette: None,
            confirm_close: None,
            status_msg: highlight_error.map(|message| (message, Instant::now())),
            git_branch: None,
            terminal: None,
            terminal_visible: std::env::var_os("KODMOZ_TERMINAL").is_some(),
            terminal_error: None,
            terminal_focus_requested: true,
            logo: load_logo_texture(&cc.egui_ctx),
            pane_layout: PaneLayout::Single,
            panes: vec![None],
            focused_pane: 0,
            markdown_cache: Default::default(),
            html_preview_cache: HashMap::new(),
            lens_texture: None,
            lens_capture_pending: false,
            lens_alt_was_down: false,
            pending_command: None,
            last_title: String::new(),
        };

        if let Some(root) = root {
            app.open_folder(root);
        }
        if let Some(layout) = std::env::var("KODMOZ_LAYOUT")
            .ok()
            .and_then(|value| PaneLayout::from_env(&value))
        {
            app.set_pane_layout(layout);
        }
        for file in files {
            app.open_file(file);
        }

        app
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status_msg = Some((message.into(), Instant::now()));
    }

    fn ensure_pane_vec(&mut self) {
        let slots = self.pane_layout.slots();
        self.panes.resize(slots, None);
        if self.focused_pane >= slots {
            self.focused_pane = slots.saturating_sub(1);
        }
    }

    fn sync_active(&mut self) {
        self.ensure_pane_vec();
        self.active = self.panes.get(self.focused_pane).copied().flatten();
    }

    fn tab_pane(&self, tab: usize) -> Option<usize> {
        self.panes.iter().position(|pane| *pane == Some(tab))
    }

    fn focus_pane(&mut self, pane: usize) {
        if pane < self.panes.len() {
            self.focused_pane = pane;
            self.sync_active();
        }
    }

    fn assign_tab_to_pane(&mut self, pane: usize, tab: Option<usize>) {
        self.ensure_pane_vec();
        let pane = pane.min(self.panes.len().saturating_sub(1));
        if let Some(tab) = tab {
            if let Some(existing) = self.tab_pane(tab) {
                if existing != pane {
                    self.focus_pane(existing);
                    return;
                }
            }
        }
        self.panes[pane] = tab;
        self.focused_pane = pane;
        self.sync_active();
    }

    fn assign_new_tab(&mut self, tab: usize) {
        self.ensure_pane_vec();
        let pane = if self
            .panes
            .get(self.focused_pane)
            .copied()
            .flatten()
            .is_none()
        {
            self.focused_pane
        } else {
            self.panes
                .iter()
                .position(|pane| pane.is_none())
                .unwrap_or_else(|| self.focused_pane.min(self.panes.len().saturating_sub(1)))
        };
        self.assign_tab_to_pane(pane, Some(tab));
    }

    fn show_tab(&mut self, tab: usize) {
        if tab >= self.tabs.len() {
            return;
        }
        self.ensure_pane_vec();
        if let Some(pane) = self.tab_pane(tab) {
            self.focus_pane(pane);
        } else {
            let pane = if self
                .panes
                .get(self.focused_pane)
                .copied()
                .flatten()
                .is_none()
            {
                self.focused_pane
            } else {
                self.panes
                    .iter()
                    .position(|pane| pane.is_none())
                    .unwrap_or_else(|| self.focused_pane.min(self.panes.len().saturating_sub(1)))
            };
            self.assign_tab_to_pane(pane, Some(tab));
        }
    }

    fn set_pane_layout(&mut self, layout: PaneLayout) {
        if self.pane_layout == layout {
            self.ensure_pane_vec();
            return;
        }
        let slots = layout.slots();
        let mut new_panes = self.panes.clone();
        new_panes.resize(slots, None);
        let mut used: HashSet<usize> = new_panes.iter().flatten().copied().collect();

        if let Some(active) = self.active {
            if !used.contains(&active) {
                if let Some(pane) = new_panes.iter().position(|pane| pane.is_none()) {
                    new_panes[pane] = Some(active);
                    used.insert(active);
                }
            }
        }
        let mut candidates: Vec<usize> = (0..self.tabs.len())
            .filter(|tab| !used.contains(tab))
            .collect();
        for pane in &mut new_panes {
            if pane.is_none() && !candidates.is_empty() {
                *pane = Some(candidates.remove(0));
            }
        }

        self.panes = new_panes;
        self.pane_layout = layout;
        self.ensure_pane_vec();
        if self
            .panes
            .get(self.focused_pane)
            .copied()
            .flatten()
            .is_none()
        {
            if let Some((pane, _)) = self
                .panes
                .iter()
                .enumerate()
                .find(|(_, pane)| pane.is_some())
            {
                self.focused_pane = pane;
            }
        }
        self.sync_active();
    }

    fn open_folder(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        if !path.is_dir() {
            self.set_status("Klasör açılamadı");
            return;
        }

        self.lsp = None;
        self.lsp_ready = false;
        self.lsp_init_id = None;
        self.lsp_tried = false;
        self.lsp_started_at = None;
        self.lsp_status = "LSP kapalı".to_string();
        self.diagnostics.clear();
        self.tabs.clear();
        self.active = None;
        self.panes = vec![None];
        self.focused_pane = 0;
        self.pane_layout = PaneLayout::Single;
        self.html_preview_cache.clear();
        self.quick_open = None;
        self.palette = None;
        self.confirm_close = None;
        self.root = Some(path.clone());
        self.explorer = Some(Explorer::new(path.clone()));
        self.git_branch = read_git_branch(&path);
        self.ensure_lsp();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        self.set_status(format!("Proje açıldı: {name}"));
    }

    fn open_file(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        if let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.path.as_deref() == Some(path.as_path()))
        {
            self.show_tab(index);
            return;
        }

        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) => {
                self.set_status(format!("Dosya açılamadı: {err}"));
                return;
            }
        };

        let mut language = LanguageId::from_path(&path);
        let (content, line_ending) = if language.is_read_only() {
            (String::new(), "\n")
        } else {
            let binary = match std::fs::File::open(&path) {
                Ok(mut file) => {
                    use std::io::Read;
                    let mut prefix = [0u8; 8192];
                    match file.read(&mut prefix) {
                        Ok(read) => prefix[..read].contains(&0),
                        Err(err) => {
                            self.set_status(format!("Dosya açılamadı: {err}"));
                            return;
                        }
                    }
                }
                Err(err) => {
                    self.set_status(format!("Dosya açılamadı: {err}"));
                    return;
                }
            };
            if binary {
                language = LanguageId::Binary;
                (String::new(), "\n")
            } else {
                let bytes = match std::fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        self.set_status(format!("Dosya açılamadı: {err}"));
                        return;
                    }
                };
                let raw = String::from_utf8_lossy(&bytes).to_string();
                if raw.contains("\r\n") {
                    (raw.replace("\r\n", "\n"), "\r\n")
                } else {
                    (raw, "\n")
                }
            }
        };

        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let hash = hash_str(&content);
        let id = self.next_tab_id;
        self.next_tab_id += 1;

        self.tabs.push(Tab {
            id,
            path: Some(path.clone()),
            name: name.clone(),
            content,
            dirty: false,
            saved_hash: hash,
            language: language.clone(),
            view_mode: default_view_mode(&language),
            version: 1,
            lsp_opened: false,
            last_sent_hash: hash,
            edit_time: None,
            pending_cursor: None,
            pending_scroll: None,
            cursor_char: 0,
            line_ending,
            file_size: Some(metadata.len()),
            image_texture: None,
            image_pixels: None,
            image_error: None,
            image_fit: true,
        });
        let index = self.tabs.len() - 1;
        self.assign_new_tab(index);

        if let Some(explorer) = &mut self.explorer {
            explorer.reveal(&path);
        }
        if language == LanguageId::Rust {
            self.ensure_lsp();
        }
        self.lsp_did_open(index);
        self.status_msg = None;
    }

    fn new_untitled(&mut self) {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let name = format!("Untitled-{id}");
        let hash = hash_str("");
        self.tabs.push(Tab {
            id,
            path: None,
            name: name.clone(),
            content: String::new(),
            dirty: false,
            saved_hash: hash,
            language: LanguageId::Plain,
            view_mode: TabViewMode::Edit,
            version: 1,
            lsp_opened: false,
            last_sent_hash: hash,
            edit_time: None,
            pending_cursor: None,
            pending_scroll: None,
            cursor_char: 0,
            line_ending: "\n",
            file_size: None,
            image_texture: None,
            image_pixels: None,
            image_error: None,
            image_fit: true,
        });
        let index = self.tabs.len() - 1;
        self.assign_new_tab(index);
        self.set_status(format!("Yeni dosya: {name}"));
    }

    fn request_close_tab(&mut self, index: usize) {
        let dirty = self.tabs.get(index).map(|tab| tab.dirty).unwrap_or(false);
        if dirty {
            self.confirm_close = Some(index);
        } else {
            self.close_tab(index);
        }
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(index);
        self.lsp_did_close(&tab);
        if let Some(path) = &tab.path {
            self.diagnostics.remove(path);
        }
        for pane in &mut self.panes {
            match *pane {
                Some(open) if open == index => *pane = None,
                Some(open) if open > index => *pane = Some(open - 1),
                _ => {}
            }
        }
        if self
            .panes
            .get(self.focused_pane)
            .copied()
            .flatten()
            .is_none()
        {
            if let Some((pane, _)) = self
                .panes
                .iter()
                .enumerate()
                .find(|(_, pane)| pane.is_some())
            {
                self.focused_pane = pane;
            }
        }
        self.sync_active();
    }

    fn save_tab(&mut self, index: usize) -> bool {
        if self
            .tabs
            .get(index)
            .map(|tab| tab.is_read_only())
            .unwrap_or(false)
        {
            self.set_status("Bu dosya salt okunur bir önizlemedir");
            return false;
        }
        let Some(path) = self.tabs.get(index).and_then(|tab| tab.path.clone()) else {
            return self.save_tab_as(index);
        };
        let name = self.tabs[index].name.clone();
        let text = self.tabs[index].serialized_content();
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.tabs[index].dirty = false;
                self.tabs[index].saved_hash = hash_str(&self.tabs[index].content);
                self.send_did_change_now(index);
                self.set_status(format!("Kaydedildi: {name}"));
                true
            }
            Err(err) => {
                self.set_status(format!("Kaydedilemedi: {err}"));
                false
            }
        }
    }

    fn save_tab_as(&mut self, index: usize) -> bool {
        if self
            .tabs
            .get(index)
            .map(|tab| tab.is_read_only())
            .unwrap_or(false)
        {
            self.set_status("Bu dosya salt okunur bir önizlemedir");
            return false;
        }
        let Some(tab) = self.tabs.get(index) else {
            return false;
        };
        let mut dialog = rfd::FileDialog::new().set_file_name(&tab.name);
        if let Some(parent) = tab.path.as_ref().and_then(|path| path.parent()) {
            dialog = dialog.set_directory(parent);
        }
        let Some(path) = dialog.save_file() else {
            return false;
        };
        let path = path.canonicalize().unwrap_or(path);
        let text = self.tabs[index].serialized_content();
        if let Err(err) = std::fs::write(&path, text) {
            self.set_status(format!("Kaydedilemedi: {err}"));
            return false;
        }

        let old_path = self.tabs[index].path.clone();
        if let (Some(client), Some(old_path)) = (&self.lsp, &old_path) {
            if let Some(uri) = path_to_uri(old_path) {
                client.notify(
                    "textDocument/didClose",
                    json!({"textDocument": {"uri": uri}}),
                );
            }
        }

        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let language = LanguageId::from_path(&path);
        let hash = hash_str(&self.tabs[index].content);
        {
            let tab = &mut self.tabs[index];
            tab.path = Some(path.clone());
            tab.name = name;
            tab.language = language.clone();
            tab.dirty = false;
            tab.saved_hash = hash;
            tab.last_sent_hash = 0;
            tab.version = 1;
            tab.lsp_opened = false;
            tab.edit_time = None;
        }
        if let Some(explorer) = &mut self.explorer {
            explorer.reveal(&path);
        }
        if language == LanguageId::Rust {
            self.ensure_lsp();
        }
        self.lsp_did_open(index);
        let saved_name = self.tabs[index].name.clone();
        self.set_status(format!("Kaydedildi: {saved_name}"));
        true
    }

    fn ensure_lsp(&mut self) {
        if self.lsp.is_some() || self.lsp_tried {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        if !root.join("Cargo.toml").is_file() {
            self.lsp_tried = true;
            self.lsp_status = "Rust projesi değil".to_string();
            return;
        }
        self.lsp_tried = true;

        let Some(binary) = find_rust_analyzer() else {
            self.lsp_status = "rust-analyzer bulunamadı".to_string();
            return;
        };
        match LspClient::spawn(&root, &binary) {
            Ok(mut client) => {
                let Some(root_uri) = path_to_uri(&root) else {
                    self.lsp_status = "Proje URI'si oluşturulamadı".to_string();
                    return;
                };
                let root_name = root
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| root.display().to_string());
                let params = json!({
                    "processId": std::process::id(),
                    "clientInfo": {
                        "name": "Kodmoz Code",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "rootUri": &root_uri,
                    "workspaceFolders": [{"uri": &root_uri, "name": root_name}],
                    "capabilities": {
                        "workspace": {"configuration": true},
                        "textDocument": {
                            "synchronization": {"didSave": false, "dynamicRegistration": false}
                        }
                    }
                });
                let init_id = client.request("initialize", params);
                self.lsp_init_id = Some(init_id);
                self.lsp = Some(client);
                self.lsp_started_at = Some(Instant::now());
                self.lsp_status = "rust-analyzer başlatılıyor...".to_string();
            }
            Err(err) => {
                self.lsp_status = format!("LSP başlatılamadı: {err}");
            }
        }
    }

    fn lsp_did_open(&mut self, index: usize) {
        if !self.lsp_ready {
            return;
        }
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        if tab.lsp_opened {
            return;
        }
        let Some(language_id) = tab.language.lsp_id() else {
            return;
        };
        let Some(path) = tab.path.clone() else {
            return;
        };
        let Some(uri) = path_to_uri(&path) else {
            return;
        };
        let content = tab.content.clone();
        let version = tab.version;
        let Some(client) = &self.lsp else {
            return;
        };
        client.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": version,
                    "text": content
                }
            }),
        );
        let hash = hash_str(&self.tabs[index].content);
        self.tabs[index].lsp_opened = true;
        self.tabs[index].last_sent_hash = hash;
    }

    fn lsp_did_close(&self, tab: &Tab) {
        if !tab.lsp_opened {
            return;
        }
        if let (Some(client), Some(path)) = (&self.lsp, &tab.path) {
            if let Some(uri) = path_to_uri(path) {
                client.notify(
                    "textDocument/didClose",
                    json!({"textDocument": {"uri": uri}}),
                );
            }
        }
    }

    fn send_did_change_now(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs[index].edit_time = None;
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        if !tab.lsp_opened {
            return;
        }
        let Some(client) = &self.lsp else {
            return;
        };
        let hash = hash_str(&tab.content);
        if hash == tab.last_sent_hash {
            return;
        }
        let Some(uri) = tab.path.as_ref().and_then(|path| path_to_uri(path)) else {
            return;
        };
        let version = tab.version + 1;
        let content = tab.content.clone();
        client.notify(
            "textDocument/didChange",
            json!({
                "textDocument": {"uri": uri, "version": version},
                "contentChanges": [{"text": content}]
            }),
        );
        self.tabs[index].version = version;
        self.tabs[index].last_sent_hash = hash;
        self.tabs[index].edit_time = None;
    }

    fn flush_lsp_changes(&mut self, ctx: &egui::Context) {
        if self.lsp.is_none() {
            return;
        }
        let mut pending = false;
        for index in 0..self.tabs.len() {
            let Some(edited_at) = self.tabs[index].edit_time else {
                continue;
            };
            if edited_at.elapsed() >= Duration::from_millis(320) {
                self.send_did_change_now(index);
            } else {
                pending = true;
            }
        }
        if pending {
            ctx.request_repaint_after(Duration::from_millis(350));
        }
    }

    fn poll_lsp(&mut self, ctx: &egui::Context) {
        let mut events = Vec::new();
        if let Some(client) = &self.lsp {
            while let Some(event) = client.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            match event {
                LspEvent::Exited(reason) => {
                    self.lsp = None;
                    self.lsp_ready = false;
                    self.lsp_status = format!("LSP kapandı: {reason}");
                }
                LspEvent::Message(message) => self.handle_lsp_message(ctx, message),
            }
        }
        if self.lsp.is_some() {
            if !self.lsp_ready
                && self
                    .lsp_started_at
                    .map(|started| started.elapsed() > Duration::from_secs(20))
                    .unwrap_or(false)
                && !self.lsp_status.contains("yanıt vermedi")
            {
                self.lsp_status = "LSP yanıt vermedi".to_string();
            }
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn handle_lsp_message(&mut self, ctx: &egui::Context, message: Value) {
        if let Some(method) = message.get("method").and_then(|method| method.as_str()) {
            if message.get("id").is_some() {
                let id = message.get("id").cloned().unwrap_or(Value::Null);
                let result = if method == "workspace/configuration" {
                    let count = message
                        .get("params")
                        .and_then(|params| params.get("items"))
                        .and_then(|items| items.as_array())
                        .map(|items| items.len())
                        .unwrap_or(0);
                    json!(vec![Value::Null; count])
                } else {
                    Value::Null
                };
                if let Some(client) = &self.lsp {
                    client.respond(id, result);
                }
                return;
            }

            match method {
                "textDocument/publishDiagnostics" => {
                    if let Some(params) = message.get("params") {
                        self.apply_diagnostics(params);
                    }
                }
                "window/logMessage" => {
                    if let Some(text) = message
                        .get("params")
                        .and_then(|params| params.get("message"))
                        .and_then(|text| text.as_str())
                    {
                        let first_line = text.lines().next().unwrap_or(text);
                        self.lsp_status = format!("RA: {}", truncate(first_line, 70));
                    }
                }
                _ => {}
            }
            return;
        }

        if let Some(id) = message.get("id").and_then(|id| id.as_i64()) {
            if Some(id) == self.lsp_init_id {
                self.lsp_ready = true;
                self.lsp_status = "rust-analyzer hazır".to_string();
                if let Some(client) = &self.lsp {
                    client.notify("initialized", json!({}));
                }
                for index in 0..self.tabs.len() {
                    if self.tabs[index].language == LanguageId::Rust {
                        self.lsp_did_open(index);
                    }
                }
                ctx.request_repaint();
            }
        }
    }

    fn apply_diagnostics(&mut self, params: &Value) {
        let Some(uri) = params.get("uri").and_then(|uri| uri.as_str()) else {
            return;
        };
        let Some(path) = uri_to_path(uri) else {
            return;
        };
        let path = path.canonicalize().unwrap_or(path);

        let mut items = Vec::new();
        if let Some(diagnostics) = params
            .get("diagnostics")
            .and_then(|diagnostics| diagnostics.as_array())
        {
            for diagnostic in diagnostics {
                let severity = diagnostic
                    .get("severity")
                    .and_then(|severity| severity.as_u64())
                    .unwrap_or(3) as u8;
                let line = diagnostic
                    .pointer("/range/start/line")
                    .and_then(|line| line.as_u64())
                    .unwrap_or(0) as u32;
                let col = diagnostic
                    .pointer("/range/start/character")
                    .and_then(|col| col.as_u64())
                    .unwrap_or(0) as u32;
                let end_line = diagnostic
                    .pointer("/range/end/line")
                    .and_then(|line| line.as_u64())
                    .unwrap_or(line as u64) as u32;
                let end_col = diagnostic
                    .pointer("/range/end/character")
                    .and_then(|col| col.as_u64())
                    .unwrap_or(col as u64) as u32;
                let message = diagnostic
                    .get("message")
                    .and_then(|message| message.as_str())
                    .unwrap_or("")
                    .to_string();
                let source = diagnostic
                    .get("source")
                    .and_then(|source| source.as_str())
                    .unwrap_or("")
                    .to_string();
                items.push(DiagnosticItem {
                    severity,
                    message,
                    line,
                    col,
                    end_line,
                    end_col,
                    source,
                });
            }
        }

        if items.is_empty() {
            self.diagnostics.remove(&path);
        } else {
            self.diagnostics.insert(path, items);
        }
    }

    fn jump_to(&mut self, path: PathBuf, line: u32, col: u32) {
        self.open_file(path);
        let Some(index) = self.active else {
            return;
        };
        if self.tabs[index].is_read_only() {
            return;
        }
        let char_index = {
            let tab = &self.tabs[index];
            char_index_for_lsp(&tab.content, line as usize, col as usize)
        };
        let line_height = self.font_size * 1.5;
        self.tabs[index].pending_cursor = Some(char_index);
        self.tabs[index].pending_scroll = Some(((line as f32) * line_height - 96.0).max(0.0));
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.quick_open.is_some() || self.palette.is_some() || self.confirm_close.is_some() {
            return;
        }

        #[derive(Clone, Copy)]
        enum Action {
            Save,
            SaveAs,
            OpenFolder,
            NewFile,
            CloseTab,
            ToggleSidebar,
            QuickOpen,
            Palette,
            ToggleTerminal,
            ToggleDiagnostics,
            ZoomIn,
            ZoomOut,
            ResetZoom,
        }

        let mut action = None;
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers {
            command: true,
            shift: true,
            ..Default::default()
        };

        ctx.input_mut(|input| {
            if input.consume_shortcut(&KeyboardShortcut::new(cmd_shift, Key::S)) {
                action = Some(Action::SaveAs);
            } else if input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::S)) {
                action = Some(Action::Save);
            }
            if input.consume_shortcut(&KeyboardShortcut::new(cmd_shift, Key::P)) {
                action = Some(Action::Palette);
            } else if input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::P)) {
                action = Some(Action::QuickOpen);
            }
            if input.consume_shortcut(&KeyboardShortcut::new(cmd_shift, Key::M)) {
                action = Some(Action::ToggleDiagnostics);
            }
            if input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Backtick))
                || input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::J))
            {
                action = Some(Action::ToggleTerminal);
            }
            if input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::O)) {
                action = Some(Action::OpenFolder);
            }
            if input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::N)) {
                action = Some(Action::NewFile);
            }
            if input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::W)) {
                action = Some(Action::CloseTab);
            }
            if input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::B)) {
                action = Some(Action::ToggleSidebar);
            }
            if input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Equals))
                || input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Plus))
            {
                action = Some(Action::ZoomIn);
            }
            if input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Minus)) {
                action = Some(Action::ZoomOut);
            }
            if input.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Num0)) {
                action = Some(Action::ResetZoom);
            }
        });

        match action {
            Some(Action::Save) => {
                if let Some(index) = self.active {
                    self.save_tab(index);
                }
            }
            Some(Action::SaveAs) => {
                if let Some(index) = self.active {
                    self.save_tab_as(index);
                }
            }
            Some(Action::OpenFolder) => self.pending_command = Some(CommandId::OpenFolder),
            Some(Action::NewFile) => self.new_untitled(),
            Some(Action::CloseTab) => {
                if let Some(index) = self.active {
                    self.request_close_tab(index);
                }
            }
            Some(Action::ToggleSidebar) => self.sidebar_visible = !self.sidebar_visible,
            Some(Action::QuickOpen) => self.open_quick_open(),
            Some(Action::Palette) => self.palette = Some(Palette::new()),
            Some(Action::ToggleTerminal) => {
                self.terminal_visible = !self.terminal_visible;
                if self.terminal_visible {
                    self.terminal_focus_requested = true;
                }
            }
            Some(Action::ToggleDiagnostics) => self.diagnostics_visible = !self.diagnostics_visible,
            Some(Action::ZoomIn) => {
                let size = (self.font_size + 1.0).min(MAX_FONT_SIZE);
                self.set_font_size(ctx, size);
            }
            Some(Action::ZoomOut) => {
                let size = (self.font_size - 1.0).max(MIN_FONT_SIZE);
                self.set_font_size(ctx, size);
            }
            Some(Action::ResetZoom) => self.set_font_size(ctx, DEFAULT_FONT_SIZE),
            None => {}
        }
    }

    fn set_font_size(&mut self, ctx: &egui::Context, size: f32) {
        let size = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        if (size - self.font_size).abs() < 0.05 {
            return;
        }
        self.font_size = size;
        ctx.style_mut(|style| {
            style
                .text_styles
                .insert(TextStyle::Monospace, FontId::monospace(size));
            style
                .text_styles
                .insert(TextStyle::Body, FontId::proportional(size * 0.93));
            style
                .text_styles
                .insert(TextStyle::Button, FontId::proportional(size * 0.86));
            style
                .text_styles
                .insert(TextStyle::Small, FontId::proportional(size * 0.71));
        });
        ctx.request_repaint();
    }

    fn update_lens(&mut self, ctx: &egui::Context) {
        let alt_down = ctx.input(|input| input.modifiers.alt);
        if alt_down && !self.lens_alt_was_down {
            self.lens_texture = None;
            if !self.lens_capture_pending {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.lens_capture_pending = true;
            }
        }
        if !alt_down && self.lens_alt_was_down {
            self.lens_capture_pending = false;
        }
        self.lens_alt_was_down = alt_down;

        let screenshot = ctx.input(|input| {
            input.events.iter().rev().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            self.lens_texture = Some(ctx.load_texture(
                "kodmoz_lens",
                (*image).clone(),
                egui::TextureOptions::LINEAR,
            ));
            self.lens_capture_pending = false;
        }

        if alt_down || self.lens_capture_pending {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn draw_lens(&self, ctx: &egui::Context) {
        if !ctx.input(|input| input.modifiers.alt) {
            return;
        }
        let Some(texture) = &self.lens_texture else {
            return;
        };
        let screen = ctx.screen_rect();
        let Some(pointer) = ctx.input(|input| input.pointer.hover_pos()) else {
            return;
        };
        if !screen.contains(pointer) {
            return;
        }
        let image_size = texture.size_vec2();
        if image_size.x < 8.0 || image_size.y < 8.0 {
            return;
        }
        let scale = Vec2::new(
            image_size.x / screen.width().max(1.0),
            image_size.y / screen.height().max(1.0),
        );
        let pointer_px = Vec2::new(
            (pointer.x - screen.left()) * scale.x,
            (pointer.y - screen.top()) * scale.y,
        );
        let uv_center = pointer_px / image_size;
        let zoom = 2.0;
        let radius = 120.0;
        let uv_radius = Vec2::new(
            radius / zoom * scale.x / image_size.x,
            radius / zoom * scale.y / image_size.y,
        );

        let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("kodmoz_lens"));
        let painter = ctx.layer_painter(layer);

        painter.circle_filled(
            pointer + Vec2::new(0.0, 3.0),
            radius + 4.0,
            Color32::from_black_alpha(70),
        );

        let segments = 64u32;
        let mut mesh = egui::Mesh::with_texture(texture.id());
        mesh.vertices.push(egui::epaint::Vertex {
            pos: pointer,
            uv: egui::pos2(uv_center.x, uv_center.y),
            color: Color32::WHITE,
        });
        for index in 0..segments {
            let angle = (index as f32 / segments as f32) * std::f32::consts::TAU;
            let direction = Vec2::new(angle.cos(), angle.sin());
            mesh.vertices.push(egui::epaint::Vertex {
                pos: pointer + direction * radius,
                uv: egui::pos2(
                    uv_center.x + direction.x * uv_radius.x,
                    uv_center.y + direction.y * uv_radius.y,
                ),
                color: Color32::WHITE,
            });
        }
        for index in 0..segments {
            mesh.add_triangle(0, 1 + index, 1 + (index + 1) % segments);
        }
        painter.add(egui::Shape::mesh(mesh));

        painter.circle_stroke(pointer, radius + 1.0, Stroke::new(2.0, theme::ACCENT));
        painter.circle_stroke(
            pointer,
            radius - 1.5,
            Stroke::new(1.0, Color32::from_black_alpha(100)),
        );

        let crosshair = Color32::from_white_alpha(160);
        painter.line_segment(
            [
                pointer + Vec2::new(-7.0, 0.0),
                pointer + Vec2::new(7.0, 0.0),
            ],
            Stroke::new(1.0, crosshair),
        );
        painter.line_segment(
            [
                pointer + Vec2::new(0.0, -7.0),
                pointer + Vec2::new(0.0, 7.0),
            ],
            Stroke::new(1.0, crosshair),
        );

        let label_center = pointer + Vec2::new(0.0, radius - 18.0);
        painter.rect_filled(
            egui::Rect::from_center_size(label_center, Vec2::new(30.0, 17.0)),
            egui::CornerRadius::same(8),
            Color32::from_black_alpha(180),
        );
        painter.text(
            label_center,
            Align2::CENTER_CENTER,
            "2×",
            FontId::proportional(10.0),
            Color32::WHITE,
        );
    }

    fn handle_pinch_zoom(&mut self, ctx: &egui::Context) {
        let delta = ctx.input(|input| input.zoom_delta());
        if (delta - 1.0).abs() < 0.001 {
            return;
        }
        let size = zoomed_font_size(self.font_size, delta);
        self.set_font_size(ctx, size);
    }

    fn open_quick_open(&mut self) {
        let Some(root) = &self.root else {
            self.set_status("Önce bir klasör açın (Cmd+O)");
            return;
        };
        self.quick_open = Some(QuickOpen::new(root.clone()));
    }

    fn apply_command(&mut self, ctx: &egui::Context, command: CommandId) {
        match command {
            CommandId::OpenFolder => {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.open_folder(folder);
                }
            }
            CommandId::NewFile => self.new_untitled(),
            CommandId::Save => {
                if let Some(index) = self.active {
                    self.save_tab(index);
                }
            }
            CommandId::SaveAs => {
                if let Some(index) = self.active {
                    self.save_tab_as(index);
                }
            }
            CommandId::CloseTab => {
                if let Some(index) = self.active {
                    self.request_close_tab(index);
                }
            }
            CommandId::QuickOpen => self.open_quick_open(),
            CommandId::Palette => self.palette = Some(Palette::new()),
            CommandId::ToggleTerminal => {
                self.terminal_visible = !self.terminal_visible;
                if self.terminal_visible {
                    self.terminal_focus_requested = true;
                }
            }
            CommandId::ToggleSidebar => self.sidebar_visible = !self.sidebar_visible,
            CommandId::ToggleDiagnostics => self.diagnostics_visible = !self.diagnostics_visible,
            CommandId::ZoomIn => {
                let size = (self.font_size + 1.0).min(MAX_FONT_SIZE);
                self.set_font_size(ctx, size);
            }
            CommandId::ZoomOut => {
                let size = (self.font_size - 1.0).max(MIN_FONT_SIZE);
                self.set_font_size(ctx, size);
            }
            CommandId::ResetZoom => self.set_font_size(ctx, DEFAULT_FONT_SIZE),
            CommandId::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ctx.request_repaint();
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("kodmoz_top")
            .exact_height(44.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::TOPBAR_BG)
                    .inner_margin(Margin::symmetric(12, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let (mark, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                    if let Some(logo) = &self.logo {
                        ui.painter().image(
                            logo.id(),
                            mark,
                            egui::Rect::from_min_max(
                                egui::pos2(0.06, 0.06),
                                egui::pos2(0.94, 0.94),
                            ),
                            Color32::WHITE,
                        );
                    } else {
                        theme::paint_brand_mark(ui.painter(), mark);
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Kodmoz")
                            .strong()
                            .size(15.0)
                            .color(theme::TEXT),
                    );
                    ui.label(RichText::new("Code").size(12.5).color(theme::ACCENT));
                    ui.add_space(4.0);
                    ui.separator();

                    if theme::icon_button(ui, theme::Icon::Folder, "Klasör Aç", "Cmd+O").clicked()
                    {
                        self.pending_command = Some(CommandId::OpenFolder);
                    }
                    if theme::icon_button(ui, theme::Icon::Save, "Kaydet", "Cmd+S").clicked() {
                        self.pending_command = Some(CommandId::Save);
                    }
                    if theme::icon_button(ui, theme::Icon::Search, "Dosyaya Git", "Cmd+P").clicked()
                    {
                        self.pending_command = Some(CommandId::QuickOpen);
                    }
                    if theme::icon_button(ui, theme::Icon::Command, "Komutlar", "Cmd+Shift+P")
                        .clicked()
                    {
                        self.pending_command = Some(CommandId::Palette);
                    }
                    if theme::icon_button(ui, theme::Icon::Terminal, "Terminal", "Cmd+`").clicked()
                    {
                        self.pending_command = Some(CommandId::ToggleTerminal);
                    }
                    if theme::icon_button(ui, theme::Icon::Problems, "Sorunlar", "Cmd+Shift+M")
                        .clicked()
                    {
                        self.pending_command = Some(CommandId::ToggleDiagnostics);
                    }
                    ui.add_space(4.0);
                    ui.separator();
                    for layout in [
                        PaneLayout::Single,
                        PaneLayout::TwoColumns,
                        PaneLayout::TwoRows,
                        PaneLayout::Three,
                        PaneLayout::Four,
                    ] {
                        let icon = match layout {
                            PaneLayout::Single => theme::Icon::LayoutSingle,
                            PaneLayout::TwoColumns => theme::Icon::LayoutColumns,
                            PaneLayout::TwoRows => theme::Icon::LayoutRows,
                            PaneLayout::Three => theme::Icon::LayoutThree,
                            PaneLayout::Four => theme::Icon::LayoutFour,
                        };
                        if theme::icon_toggle_button(
                            ui,
                            icon,
                            self.pane_layout == layout,
                            layout.tooltip(),
                        )
                        .clicked()
                        {
                            self.set_pane_layout(layout);
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(root) = &self.root {
                            let name = root
                                .file_name()
                                .map(|name| name.to_string_lossy().to_string())
                                .unwrap_or_else(|| root.display().to_string());
                            let chip = egui::Frame::NONE
                                .fill(theme::SURFACE)
                                .corner_radius(6)
                                .inner_margin(Margin::symmetric(8, 3))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(name).size(10.0).color(theme::MUTED));
                                });
                            chip.response.on_hover_text(root.display().to_string());

                            ui.add_space(10.0);
                            ui.label(
                                RichText::new(truncate(&self.lsp_status, 34))
                                    .size(9.5)
                                    .color(theme::FAINT),
                            );
                            let color = if self.lsp_ready {
                                theme::GREEN
                            } else if self.lsp.is_some() {
                                theme::YELLOW
                            } else if self.lsp_status.contains("bulunamadı")
                                || self.lsp_status.contains("kapandı")
                            {
                                theme::RED
                            } else {
                                theme::FAINT
                            };
                            theme::status_dot(ui, color).on_hover_text(&self.lsp_status);
                        }
                    });
                });
            });
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        let mut refresh = false;
        egui::SidePanel::left("kodmoz_explorer")
            .resizable(true)
            .default_width(260.0)
            .min_width(180.0)
            .max_width(520.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDEBAR_BG)
                    .inner_margin(Margin::symmetric(8, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                    theme::paint_icon(ui.painter(), icon_rect, theme::Icon::Folder, theme::FAINT);
                    ui.label(
                        RichText::new("EXPLORER")
                            .size(9.5)
                            .strong()
                            .color(theme::FAINT),
                    );
                    ui.add_space((ui.available_width() - 52.0).max(0.0));
                    if theme::icon_only_button(ui, theme::Icon::FilePlus, "Yeni dosya").clicked() {
                        self.pending_command = Some(CommandId::NewFile);
                    }
                    if theme::icon_only_button(ui, theme::Icon::Refresh, "Yenile").clicked() {
                        refresh = true;
                    }
                });
                ui.add_space(2.0);
                ui.separator();
                ui.add_space(2.0);

                let active = self
                    .active
                    .and_then(|index| self.tabs.get(index))
                    .and_then(|tab| tab.path.clone());
                let mut clicked = None;
                if let Some(explorer) = &mut self.explorer {
                    clicked = explorer.ui(ui, active.as_deref());
                } else {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Proje açılmadı")
                            .size(11.0)
                            .color(theme::MUTED),
                    );
                    ui.add_space(4.0);
                    if theme::icon_button(ui, theme::Icon::Folder, "Klasör Aç", "").clicked() {
                        self.pending_command = Some(CommandId::OpenFolder);
                    }
                }
                if let Some(path) = clicked {
                    self.open_file(path);
                }
            });
        if refresh {
            if let Some(explorer) = &mut self.explorer {
                explorer.refresh();
            }
        }
    }

    fn diagnostics_panel(&mut self, ctx: &egui::Context) {
        let mut rows: Vec<(PathBuf, DiagnosticItem)> = Vec::new();
        for (path, items) in &self.diagnostics {
            for item in items {
                rows.push((path.clone(), item.clone()));
            }
        }
        rows.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.line.cmp(&b.1.line))
                .then_with(|| a.1.col.cmp(&b.1.col))
        });
        let errors = rows.iter().filter(|(_, item)| item.is_error()).count();
        let warnings = rows.iter().filter(|(_, item)| item.is_warning()).count();

        let mut jump = None;
        let mut close = false;
        egui::TopBottomPanel::bottom("kodmoz_problems")
            .resizable(true)
            .default_height(170.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::PANEL_BG)
                    .inner_margin(Margin::symmetric(10, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                    theme::paint_icon(ui.painter(), icon_rect, theme::Icon::Problems, theme::FAINT);
                    ui.label(
                        RichText::new("SORUNLAR")
                            .size(9.5)
                            .strong()
                            .color(theme::FAINT),
                    );
                    let (color, background) = if errors > 0 {
                        (theme::RED, theme::RED_BG)
                    } else if warnings > 0 {
                        (theme::YELLOW, theme::YELLOW_BG)
                    } else {
                        (theme::GREEN, theme::SURFACE)
                    };
                    theme::chip(ui, &format!("{} sorun", rows.len()), color, background);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::close_button(ui).clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("kodmoz_problems_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if rows.is_empty() {
                            ui.add_space(8.0);
                            ui.label(RichText::new("Sorun yok").size(11.0).color(theme::FAINT));
                        }
                        for (path, item) in &rows {
                            let name = self
                                .root
                                .as_ref()
                                .and_then(|root| path.strip_prefix(root).ok())
                                .map(|relative| relative.display().to_string())
                                .unwrap_or_else(|| path.display().to_string());
                            let text = format!("{}   ·   {}:{}", item.message, name, item.line + 1);
                            if diagnostic_row(ui, &text, item.severity).clicked() {
                                jump = Some((path.clone(), item.line, item.col));
                            }
                        }
                    });
            });

        if close {
            self.diagnostics_visible = false;
        }
        if let Some((path, line, col)) = jump {
            self.jump_to(path, line, col);
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let (errors, warnings) = self.active_diagnostic_counts();
        let tab = self.active.and_then(|index| self.tabs.get(index));
        let language = tab.map(|tab| tab.language.label()).unwrap_or("—");
        let is_text = tab.map(|tab| !tab.is_read_only()).unwrap_or(false);
        let is_read_only = tab.map(|tab| tab.is_read_only()).unwrap_or(false);
        let position = match tab {
            Some(tab) if tab.language == LanguageId::Image => tab
                .image_pixels
                .map(|(width, height)| format!("{width} × {height}"))
                .unwrap_or_else(|| "önizleme".to_string()),
            Some(tab) if tab.language == LanguageId::Binary => "ikili dosya".to_string(),
            _ => {
                let (line, column) = self.cursor_line_col();
                format!("Ln {line}, Col {column}")
            }
        };
        let eol = tab
            .map(|tab| {
                if tab.line_ending == "\r\n" {
                    "CRLF"
                } else {
                    "LF"
                }
            })
            .unwrap_or("LF");
        let encoding = if is_text { "UTF-8" } else { "—" };
        let message = self
            .status_msg
            .as_ref()
            .map(|(message, _)| message.clone())
            .or_else(|| {
                tab.and_then(|tab| {
                    tab.path.as_ref().map(|path| {
                        self.root
                            .as_ref()
                            .and_then(|root| path.strip_prefix(root).ok())
                            .map(|relative| relative.display().to_string())
                            .unwrap_or_else(|| {
                                path.file_name()
                                    .map(|name| name.to_string_lossy().to_string())
                                    .unwrap_or_else(|| path.display().to_string())
                            })
                    })
                })
            })
            .or_else(|| {
                self.root.as_ref().map(|root| {
                    root.file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| root.display().to_string())
                })
            })
            .unwrap_or_else(|| "Kodmoz Code".to_string());
        let branch = self.git_branch.clone();

        egui::TopBottomPanel::bottom("kodmoz_status")
            .exact_height(26.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::STATUS_BG)
                    .inner_margin(Margin::symmetric(10, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                    ui.painter().circle_filled(dot.center(), 2.5, theme::ACCENT);
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(truncate(&message, 100))
                            .size(9.5)
                            .color(theme::MUTED),
                    );
                    if let Some(branch) = &branch {
                        ui.add_space(8.0);
                        let (icon_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(11.0), Sense::hover());
                        theme::paint_icon(
                            ui.painter(),
                            icon_rect,
                            theme::Icon::Branch,
                            theme::FAINT,
                        );
                        ui.label(
                            RichText::new(format!("git:{branch}"))
                                .size(9.5)
                                .color(theme::FAINT),
                        );
                    }
                    if is_read_only {
                        ui.add_space(8.0);
                        let (icon_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                        theme::paint_icon(ui.painter(), icon_rect, theme::Icon::Lock, theme::FAINT);
                        ui.label(RichText::new("salt okunur").size(9.0).color(theme::FAINT));
                    }
                    if errors > 0 {
                        ui.add_space(8.0);
                        let (icon_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                        theme::paint_icon(
                            ui.painter(),
                            icon_rect,
                            theme::Icon::Problems,
                            theme::RED,
                        );
                        if theme::chip(ui, &format!("{errors} hata"), theme::RED, theme::RED_BG)
                            .on_hover_text("Sorunlar panelini aç/kapat")
                            .clicked()
                        {
                            self.pending_command = Some(CommandId::ToggleDiagnostics);
                        }
                    }
                    if warnings > 0 {
                        ui.add_space(6.0);
                        if theme::chip(
                            ui,
                            &format!("{warnings} uyarı"),
                            theme::YELLOW,
                            theme::YELLOW_BG,
                        )
                        .on_hover_text("Sorunlar panelini aç/kapat")
                        .clicked()
                        {
                            self.pending_command = Some(CommandId::ToggleDiagnostics);
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(10.0);
                        ui.label(RichText::new(&position).size(9.5).color(theme::TEXT));
                        if tab.is_some() {
                            status_separator(ui);
                            ui.label(RichText::new(eol).size(9.5).color(theme::FAINT));
                            status_separator(ui);
                            ui.label(RichText::new(encoding).size(9.5).color(theme::FAINT));
                            status_separator(ui);
                            ui.label(RichText::new(language).size(9.5).color(theme::MUTED));
                        }
                    });
                });
            });
    }

    fn active_diagnostic_counts(&self) -> (usize, usize) {
        let Some(path) = self
            .active
            .and_then(|index| self.tabs.get(index))
            .and_then(|tab| tab.path.as_ref())
        else {
            return (0, 0);
        };
        let Some(items) = self.diagnostics.get(path) else {
            return (0, 0);
        };
        let errors = items.iter().filter(|item| item.is_error()).count();
        let warnings = items.iter().filter(|item| item.is_warning()).count();
        (errors, warnings)
    }

    fn cursor_line_col(&self) -> (usize, usize) {
        let Some(tab) = self.active.and_then(|index| self.tabs.get(index)) else {
            return (1, 1);
        };
        let (line, column) = line_col_of_char(&tab.content, tab.cursor_char);
        (line + 1, column + 1)
    }

    fn central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG))
            .show(ctx, |ui| {
                if self.tabs.is_empty() {
                    self.welcome(ui);
                } else {
                    self.tabs_bar(ui);
                    self.panes_ui(ui);
                }
            });
    }

    fn panes_ui(&mut self, ui: &mut Ui) {
        self.ensure_pane_vec();
        let available = ui.available_size();
        let gap = 4.0;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let mut clicked_pane = None;

        match self.pane_layout {
            PaneLayout::Single => {
                if self.pane_ui(ui, 0, available) {
                    clicked_pane = Some(0);
                }
            }
            PaneLayout::TwoColumns => {
                let width = ((available.x - gap) * 0.5).max(80.0);
                ui.horizontal(|ui| {
                    if self.pane_ui(ui, 0, Vec2::new(width, available.y)) {
                        clicked_pane = Some(0);
                    }
                    ui.add_space(gap);
                    if self.pane_ui(
                        ui,
                        1,
                        Vec2::new((available.x - width - gap).max(80.0), available.y),
                    ) {
                        clicked_pane = Some(1);
                    }
                });
            }
            PaneLayout::TwoRows => {
                let height = ((available.y - gap) * 0.5).max(80.0);
                ui.vertical(|ui| {
                    if self.pane_ui(ui, 0, Vec2::new(available.x, height)) {
                        clicked_pane = Some(0);
                    }
                    ui.add_space(gap);
                    if self.pane_ui(
                        ui,
                        1,
                        Vec2::new(available.x, (available.y - height - gap).max(80.0)),
                    ) {
                        clicked_pane = Some(1);
                    }
                });
            }
            PaneLayout::Three => {
                let width = ((available.x - gap * 2.0) / 3.0).max(80.0);
                ui.horizontal(|ui| {
                    for pane in 0..3 {
                        if self.pane_ui(ui, pane, Vec2::new(width, available.y)) {
                            clicked_pane = Some(pane);
                        }
                        if pane < 2 {
                            ui.add_space(gap);
                        }
                    }
                });
            }
            PaneLayout::Four => {
                let width = ((available.x - gap) * 0.5).max(80.0);
                let height = ((available.y - gap) * 0.5).max(80.0);
                let bottom_height = (available.y - height - gap).max(80.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if self.pane_ui(ui, 0, Vec2::new(width, height)) {
                            clicked_pane = Some(0);
                        }
                        ui.add_space(gap);
                        if self.pane_ui(
                            ui,
                            1,
                            Vec2::new((available.x - width - gap).max(80.0), height),
                        ) {
                            clicked_pane = Some(1);
                        }
                    });
                    ui.add_space(gap);
                    ui.horizontal(|ui| {
                        if self.pane_ui(ui, 2, Vec2::new(width, bottom_height)) {
                            clicked_pane = Some(2);
                        }
                        ui.add_space(gap);
                        if self.pane_ui(
                            ui,
                            3,
                            Vec2::new((available.x - width - gap).max(80.0), bottom_height),
                        ) {
                            clicked_pane = Some(3);
                        }
                    });
                });
            }
        }

        if let Some(pane) = clicked_pane {
            self.focus_pane(pane);
        }
    }

    fn pane_ui(&mut self, ui: &mut Ui, pane_index: usize, size: Vec2) -> bool {
        let response = ui.allocate_ui_with_layout(size, Layout::top_down(Align::Min), |ui| {
            let focused = pane_index == self.focused_pane;
            let border = if focused {
                theme::ACCENT.gamma_multiply(0.55)
            } else {
                theme::BORDER
            };
            egui::Frame::NONE
                .fill(theme::BG)
                .stroke(Stroke::new(1.0, border))
                .show(ui, |ui| {
                    ui.set_min_size(Vec2::new((size.x - 2.0).max(0.0), (size.y - 2.0).max(0.0)));
                    self.pane_header(ui, pane_index);
                    if let Some(tab) = self.panes.get(pane_index).copied().flatten() {
                        self.show_tab_ui(ui, tab);
                    } else {
                        self.empty_pane_ui(ui);
                    }
                });
        });

        let pane_rect = response.response.rect;
        let pressed = ui.ctx().input(|input| input.pointer.any_pressed());
        let inside = ui
            .ctx()
            .input(|input| input.pointer.interact_pos())
            .map(|position| pane_rect.contains(position))
            .unwrap_or(false);
        pressed && inside
    }

    fn pane_header(&mut self, ui: &mut Ui, pane_index: usize) {
        let focused = pane_index == self.focused_pane;
        let tab_index = self.panes.get(pane_index).copied().flatten();

        let Some(tab_index) = tab_index else {
            let header = egui::Frame::NONE
                .fill(theme::SURFACE)
                .inner_margin(Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (icon_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(11.0), Sense::hover());
                        theme::paint_icon(
                            ui.painter(),
                            icon_rect,
                            theme::Icon::FilePlus,
                            theme::FAINT,
                        );
                        ui.label(RichText::new("Boş bölme").size(10.0).color(theme::FAINT));
                    });
                });
            let rect = header.response.rect;
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left(), rect.bottom()),
                    egui::pos2(rect.right(), rect.bottom()),
                ],
                Stroke::new(1.0, theme::BORDER),
            );
            return;
        };

        if tab_index >= self.tabs.len() {
            self.panes[pane_index] = None;
            return;
        }

        let name = self.tabs[tab_index].name.clone();
        let dirty = self.tabs[tab_index].dirty;
        let language = self.tabs[tab_index].language.clone();
        let mode = self.tabs[tab_index].view_mode;
        let extension = self.tabs[tab_index]
            .path
            .as_ref()
            .and_then(|path| path.extension())
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_string();
        let mut new_mode = None;
        let mut close = false;

        let header = egui::Frame::NONE
            .fill(theme::SURFACE)
            .inner_margin(Margin::symmetric(7, 3))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.horizontal(|ui| {
                    let (badge, _) = ui.allocate_exact_size(Vec2::new(18.0, 11.0), Sense::hover());
                    theme::paint_file_badge(ui.painter(), badge, &extension);
                    ui.label(
                        RichText::new(truncate(&name, 22))
                            .size(10.5)
                            .color(if focused { theme::TEXT } else { theme::MUTED }),
                    )
                    .on_hover_text(&name);
                    if dirty {
                        let (dot, _) = ui.allocate_exact_size(Vec2::splat(6.0), Sense::hover());
                        ui.painter().circle_filled(dot.center(), 2.2, theme::ACCENT);
                    }

                    let preview = language.supports_preview();
                    let needed = if preview { 106.0 } else { 24.0 };
                    ui.add_space((ui.available_width() - needed).max(0.0));
                    if preview {
                        if theme::icon_toggle_button(
                            ui,
                            theme::Icon::Edit,
                            mode == TabViewMode::Edit,
                            "Düzenle",
                        )
                        .clicked()
                        {
                            new_mode = Some(TabViewMode::Edit);
                        }
                        if theme::icon_toggle_button(
                            ui,
                            theme::Icon::Split,
                            mode == TabViewMode::Split,
                            "Bölünmüş görünüm",
                        )
                        .clicked()
                        {
                            new_mode = Some(TabViewMode::Split);
                        }
                        if theme::icon_toggle_button(
                            ui,
                            theme::Icon::Eye,
                            mode == TabViewMode::Preview,
                            "Önizleme",
                        )
                        .clicked()
                        {
                            new_mode = Some(TabViewMode::Preview);
                        }
                    }
                    if theme::close_button(ui).clicked() {
                        close = true;
                    }
                });
            });

        let rect = header.response.rect;
        ui.painter().line_segment(
            [
                egui::pos2(rect.left(), rect.bottom()),
                egui::pos2(rect.right(), rect.bottom()),
            ],
            Stroke::new(
                1.0,
                if focused {
                    theme::ACCENT
                } else {
                    theme::BORDER
                },
            ),
        );

        if let Some(mode) = new_mode {
            self.tabs[tab_index].view_mode = mode;
        }
        if close {
            self.request_close_tab(tab_index);
        }
    }

    fn empty_pane_ui(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.25).max(24.0));
            let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
            theme::paint_icon(ui.painter(), icon_rect, theme::Icon::FilePlus, theme::FAINT);
            ui.add_space(8.0);
            ui.label(RichText::new("Boş bölme").size(11.5).color(theme::MUTED));
            ui.add_space(6.0);
            if theme::icon_button(ui, theme::Icon::Search, "Dosyaya Git", "Cmd+P").clicked() {
                self.pending_command = Some(CommandId::QuickOpen);
            }
        });
    }

    fn tabs_bar(&mut self, ui: &mut Ui) {
        let infos: Vec<(usize, u64, String, bool, bool, String)> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let extension = tab
                    .path
                    .as_ref()
                    .and_then(|path| path.extension())
                    .and_then(|extension| extension.to_str())
                    .unwrap_or("")
                    .to_string();
                (
                    index,
                    tab.id,
                    tab.name.clone(),
                    tab.dirty,
                    Some(index) == self.active,
                    extension,
                )
            })
            .collect();
        let mut activate = None;
        let mut close = None;
        let mut new_file = false;
        let mut active_rect = None;
        let font = FontId::proportional(12.0);

        let bar = egui::Frame::NONE
            .fill(theme::TABBAR_BG)
            .inner_margin(Margin {
                left: 6,
                right: 6,
                top: 6,
                bottom: 0,
            })
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("kodmoz_tabs")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            for (index, tab_id, name, dirty, selected, extension) in &infos {
                                let text_width = ui.fonts(|fonts| {
                                    fonts
                                        .layout_no_wrap(name.clone(), font.clone(), theme::TEXT)
                                        .size()
                                        .x
                                });
                                let width = (text_width + 73.0).clamp(120.0, 290.0);
                                let (rect, response) =
                                    ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::click());
                                let hovered = response.hovered();
                                let painter = ui.painter().with_clip_rect(rect);
                                let top_radius = egui::CornerRadius {
                                    nw: 7,
                                    ne: 7,
                                    sw: 0,
                                    se: 0,
                                };

                                if *selected {
                                    active_rect = Some(rect);
                                    painter.rect_filled(rect, top_radius, theme::BG);
                                    painter.rect_stroke(
                                        rect,
                                        top_radius,
                                        Stroke::new(1.0, theme::BORDER),
                                        egui::StrokeKind::Inside,
                                    );
                                    painter.rect_filled(
                                        egui::Rect::from_min_size(
                                            rect.min,
                                            Vec2::new(rect.width(), 2.0),
                                        ),
                                        egui::CornerRadius {
                                            nw: 2,
                                            ne: 2,
                                            sw: 0,
                                            se: 0,
                                        },
                                        theme::ACCENT,
                                    );
                                } else {
                                    let fill = if hovered {
                                        theme::SURFACE_HI
                                    } else {
                                        theme::SURFACE
                                    };
                                    painter.rect_filled(rect, top_radius, fill);
                                    painter.rect_stroke(
                                        rect,
                                        top_radius,
                                        Stroke::new(1.0, theme::BORDER),
                                        egui::StrokeKind::Inside,
                                    );
                                }

                                let badge = egui::Rect::from_min_size(
                                    egui::pos2(rect.left() + 11.0, rect.center().y - 6.5),
                                    Vec2::new(22.0, 13.0),
                                );
                                theme::paint_file_badge(&painter, badge, extension);
                                painter.text(
                                    egui::pos2(rect.left() + 39.0, rect.center().y),
                                    Align2::LEFT_CENTER,
                                    name,
                                    font.clone(),
                                    if *selected { theme::TEXT } else { theme::MUTED },
                                );

                                let close_rect = egui::Rect::from_center_size(
                                    egui::pos2(rect.right() - 16.0, rect.center().y),
                                    Vec2::splat(18.0),
                                );
                                let close_response = ui.interact(
                                    close_rect,
                                    ui.id().with(("kodmoz_tab_close", *tab_id)),
                                    Sense::click(),
                                );
                                let show_close = *selected || hovered || close_response.hovered();
                                if *dirty {
                                    let dot_x = if show_close {
                                        close_rect.left() - 7.0
                                    } else {
                                        close_rect.center().x
                                    };
                                    painter.circle_filled(
                                        egui::pos2(dot_x, rect.center().y),
                                        2.5,
                                        theme::ACCENT,
                                    );
                                }
                                if show_close {
                                    let color = if close_response.hovered() {
                                        theme::TEXT
                                    } else {
                                        theme::FAINT
                                    };
                                    theme::paint_close_x(&painter, close_rect.shrink(5.5), color);
                                }

                                if close_response.clicked() {
                                    close = Some(*index);
                                } else if response.clicked() {
                                    activate = Some(*index);
                                } else if response.middle_clicked() {
                                    close = Some(*index);
                                }
                            }
                            if theme::icon_only_button(ui, theme::Icon::Plus, "Yeni dosya")
                                .clicked()
                            {
                                new_file = true;
                            }
                        });
                    });
            });

        let rect = bar.response.rect;
        let painter = ui.painter();
        painter.line_segment(
            [
                egui::pos2(rect.left(), rect.bottom()),
                egui::pos2(rect.right(), rect.bottom()),
            ],
            Stroke::new(1.0, theme::BORDER),
        );
        if let Some(active) = active_rect {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(active.left() + 1.0, rect.bottom() - 1.0),
                    egui::pos2(active.right() - 1.0, rect.bottom() + 1.0),
                ),
                egui::CornerRadius::ZERO,
                theme::BG,
            );
        }

        if let Some(index) = activate {
            self.show_tab(index);
        }
        if let Some(index) = close {
            self.request_close_tab(index);
        }
        if new_file {
            self.new_untitled();
        }
    }

    fn welcome(&mut self, ui: &mut Ui) {
        theme::paint_dot_grid(ui);
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.12).max(30.0));
            let (mark, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
            if let Some(logo) = &self.logo {
                ui.painter().image(
                    logo.id(),
                    mark,
                    egui::Rect::from_min_max(egui::pos2(0.06, 0.06), egui::pos2(0.94, 0.94)),
                    Color32::WHITE,
                );
            } else {
                theme::paint_brand_mark(ui.painter(), mark);
            }
            ui.add_space(14.0);
            ui.label(
                RichText::new("Kodmoz Code")
                    .size(32.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("Rust ile yazılmış, LSP destekli minimal IDE")
                    .size(12.5)
                    .color(theme::MUTED),
            );
            ui.add_space(22.0);
            let button = egui::Button::new(
                RichText::new("Klasör Aç...")
                    .size(12.5)
                    .color(Color32::from_rgb(0x1A, 0x14, 0x0A)),
            )
            .fill(theme::ACCENT)
            .corner_radius(6)
            .min_size(Vec2::new(150.0, 32.0));
            if ui.add(button).clicked() {
                self.pending_command = Some(CommandId::OpenFolder);
            }
            ui.add_space(28.0);

            let cards_width = 4.0 * 178.0 + 3.0 * 10.0;
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - cards_width) * 0.5).max(0.0));
                welcome_card(ui, theme::Icon::Folder, "Klasör Aç", "Cmd+O");
                welcome_card(ui, theme::Icon::Search, "Dosyaya Git", "Cmd+P");
                welcome_card(ui, theme::Icon::Command, "Komut Paleti", "Cmd+Shift+P");
                welcome_card(ui, theme::Icon::Terminal, "Terminal", "Cmd+`");
            });
            ui.add_space(20.0);
            ui.label(
                RichText::new("Option (Alt) basılı tut: 2× mercek  ·  Cmd+0: yazı boyutu sıfırla")
                    .size(10.0)
                    .color(theme::FAINT),
            );
        });
    }

    fn show_dialogs(&mut self, ctx: &egui::Context) {
        let quick_open_result = self.quick_open.as_mut().map(|dialog| dialog.show(ctx));
        match quick_open_result {
            Some(QuickOpenResult::Keep) | None => {}
            Some(QuickOpenResult::Close) => self.quick_open = None,
            Some(QuickOpenResult::Open(path)) => {
                self.quick_open = None;
                self.open_file(path);
            }
        }

        let palette_result = self.palette.as_mut().map(|palette| palette.show(ctx));
        match palette_result {
            Some(PaletteResult::Keep) | None => {}
            Some(PaletteResult::Close) => self.palette = None,
            Some(PaletteResult::Run(command)) => {
                self.palette = None;
                self.pending_command = Some(command);
            }
        }

        if let Some(index) = self.confirm_close {
            let name = self
                .tabs
                .get(index)
                .map(|tab| tab.name.clone())
                .unwrap_or_default();
            let mut choice = None;
            egui::Window::new("Kaydedilmemiş değişiklikler")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!("'{name}' dosyası kaydedilsin mi?"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Kaydet").clicked() {
                            choice = Some(0);
                        }
                        if ui.button("Kaydetme").clicked() {
                            choice = Some(1);
                        }
                        if ui.button("İptal").clicked() {
                            choice = Some(2);
                        }
                    });
                });

            match choice {
                Some(0) => {
                    if self.save_tab(index) {
                        self.close_tab(index);
                    }
                    self.confirm_close = None;
                }
                Some(1) => {
                    self.close_tab(index);
                    self.confirm_close = None;
                }
                Some(2) => self.confirm_close = None,
                _ => {}
            }
        }
    }

    fn open_system_path(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        if path.is_dir() {
            self.open_folder(path);
            return;
        }
        if !path.is_file() {
            return;
        }
        if self.root.is_none() {
            self.open_folder(project_root_for(&path));
        }
        self.open_file(path);
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        for path in dropped {
            if path.is_dir() {
                self.open_folder(path);
            } else if path.is_file() {
                self.open_file(path);
            }
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = if let Some(index) = self.active {
            let tab = &self.tabs[index];
            format!(
                "{}{} — Kodmoz Code",
                if tab.dirty { "* " } else { "" },
                tab.name
            )
        } else {
            "Kodmoz Code".to_string()
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    fn expire_status(&mut self, ctx: &egui::Context) {
        if let Some((_, created_at)) = &self.status_msg {
            if created_at.elapsed() > Duration::from_secs(6) {
                self.status_msg = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
    }
}

impl eframe::App for KodmozCode {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_lens(ctx);
        self.handle_pinch_zoom(ctx);
        self.handle_shortcuts(ctx);
        for path in crate::macos_open::take() {
            self.open_system_path(path);
        }
        self.handle_dropped_files(ctx);
        self.poll_lsp(ctx);
        self.flush_lsp_changes(ctx);

        self.top_bar(ctx);
        self.status_bar(ctx);
        if self.terminal_visible {
            self.terminal_panel(ctx);
        }
        if self.diagnostics_visible {
            self.diagnostics_panel(ctx);
        }
        if self.sidebar_visible {
            self.sidebar(ctx);
        }
        self.central(ctx);

        self.show_dialogs(ctx);

        if let Some(command) = self.pending_command.take() {
            self.apply_command(ctx, command);
        }

        self.update_title(ctx);
        self.expire_status(ctx);
        self.draw_lens(ctx);
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

fn default_view_mode(language: &LanguageId) -> TabViewMode {
    if !language.supports_preview() {
        return TabViewMode::Edit;
    }
    match std::env::var("KODMOZ_PREVIEW").as_deref() {
        Ok("edit") => TabViewMode::Edit,
        Ok("preview") => TabViewMode::Preview,
        Ok("split") => TabViewMode::Split,
        _ => {
            if language.is_html() {
                TabViewMode::Preview
            } else {
                TabViewMode::Split
            }
        }
    }
}

fn load_logo_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let bytes = include_bytes!("../assets/icon-1024.png");
    let image = image::load_from_memory(bytes).ok()?.thumbnail(128, 128);
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture("kodmoz-logo", color_image, egui::TextureOptions::LINEAR))
}

fn configure_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = theme::SIDEBAR_BG;
    visuals.window_fill = theme::SURFACE;
    visuals.extreme_bg_color = theme::BG;
    visuals.faint_bg_color = theme::SURFACE;
    visuals.selection.bg_fill = Color32::from_rgb(0x2C, 0x3E, 0x5C);
    visuals.selection.stroke = Stroke::new(1.0, theme::ACCENT);
    visuals.window_corner_radius = egui::CornerRadius::same(8);
    visuals.menu_corner_radius = egui::CornerRadius::same(8);
    visuals.override_text_color = Some(theme::TEXT);

    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, theme::BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, theme::MUTED);

    visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, theme::TEXT);

    visuals.widgets.hovered.weak_bg_fill = theme::SURFACE_HI;
    visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, theme::TEXT);

    visuals.widgets.active.weak_bg_fill = theme::SURFACE_HI;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, theme::BORDER_STRONG);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, theme::TEXT);

    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(6);
    }

    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 5.0);
        style.spacing.button_padding = Vec2::new(9.0, 4.0);
        style.spacing.scroll.bar_width = 8.0;
        style.spacing.scroll.floating = true;
        style.spacing.scroll.floating_width = 4.0;
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(14.0));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(12.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(10.0));
    });
}

fn welcome_card(ui: &mut Ui, icon: theme::Icon, title: &str, shortcut: &str) {
    egui::Frame::NONE
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(150.0);
            ui.horizontal(|ui| {
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                theme::paint_icon(ui.painter(), icon_rect, icon, theme::ACCENT);
                ui.label(RichText::new(title).size(11.5).color(theme::TEXT));
            });
            ui.add_space(10.0);
            theme::key_chip(ui, shortcut);
        });
}

fn diagnostic_row(ui: &mut Ui, text: &str, severity: u8) -> egui::Response {
    let color = severity_color(severity);
    let font = FontId::proportional(11.0);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 21.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), theme::SURFACE);
    }
    let painter = ui.painter().with_clip_rect(rect);
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 9.0, rect.center().y),
        Vec2::splat(11.0),
    );
    theme::paint_severity_icon(&painter, icon_rect, severity);
    let galley = ui.fonts(|fonts| fonts.layout_no_wrap(text.to_owned(), font, color));
    let text_pos = egui::pos2(rect.left() + 19.0, rect.center().y - galley.size().y * 0.5);
    painter.galley(text_pos, galley, color);
    response
}

fn status_separator(ui: &mut Ui) {
    ui.label(RichText::new("·").size(9.0).color(theme::FAINT));
}

fn zoomed_font_size(current: f32, delta: f32) -> f32 {
    (current * delta).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
}

fn project_root_for(file: &Path) -> PathBuf {
    let start = file.parent().unwrap_or(file);
    for ancestor in start.ancestors() {
        if ancestor.join("Cargo.toml").is_file() || ancestor.join(".git").exists() {
            return ancestor.to_path_buf();
        }
    }
    start.to_path_buf()
}

fn read_git_branch(root: &Path) -> Option<String> {
    let head = std::fs::read_to_string(root.join(".git/HEAD")).ok()?;
    let head = head.trim();
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        Some(branch.to_string())
    } else if head.len() >= 7 {
        Some(head.chars().take(7).collect())
    } else {
        None
    }
}

pub(crate) fn hash_str(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn line_col_of_char(text: &str, char_index: usize) -> (usize, usize) {
    let mut line = 0;
    let mut column = 0;
    for (index, ch) in text.chars().enumerate() {
        if index >= char_index {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    (line, column)
}

pub(crate) fn line_of_char(text: &str, char_index: usize) -> usize {
    text.chars()
        .take(char_index)
        .filter(|ch| *ch == '\n')
        .count()
}

fn line_start_char_index(text: &str, target_line: usize) -> usize {
    let mut line = 0;
    let mut index = 0;
    for ch in text.chars() {
        if line >= target_line {
            break;
        }
        index += 1;
        if ch == '\n' {
            line += 1;
        }
    }
    index
}

pub(crate) fn char_index_for_lsp(text: &str, line: usize, utf16_col: usize) -> usize {
    let start = line_start_char_index(text, line);
    let mut char_index = start;
    let mut utf16_count = 0;
    for ch in text.chars().skip(start) {
        if ch == '\n' || utf16_count >= utf16_col {
            break;
        }
        utf16_count += ch.len_utf16();
        char_index += 1;
    }
    char_index
}

pub(crate) fn severity_label(severity: u8) -> &'static str {
    match severity {
        1 => "Hata",
        2 => "Uyarı",
        3 => "Bilgi",
        _ => "İpucu",
    }
}

pub(crate) fn severity_color(severity: u8) -> Color32 {
    match severity {
        1 => theme::RED,
        2 => theme::YELLOW,
        3 => theme::BLUE,
        _ => theme::FAINT,
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut result: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        result.push('…');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_and_column_are_zero_based() {
        assert_eq!(line_col_of_char("ab\ncd", 0), (0, 0));
        assert_eq!(line_col_of_char("ab\ncd", 2), (0, 2));
        assert_eq!(line_col_of_char("ab\ncd", 3), (1, 0));
        assert_eq!(line_col_of_char("ab\ncd", 4), (1, 1));
    }

    #[test]
    fn line_of_char_counts_newlines() {
        assert_eq!(line_of_char("ab\ncd", 0), 0);
        assert_eq!(line_of_char("ab\ncd", 3), 1);
        assert_eq!(line_of_char("ab\ncd\nef", 9), 2);
    }

    #[test]
    fn pinch_zoom_is_clamped() {
        assert!((zoomed_font_size(14.0, 1.2) - 16.8).abs() < 0.001);
        assert_eq!(zoomed_font_size(14.0, 5.0), MAX_FONT_SIZE);
        assert_eq!(zoomed_font_size(14.0, 0.1), MIN_FONT_SIZE);
    }

    #[test]
    fn egui_reports_zoom_events() {
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Zoom(1.25)],
                ..Default::default()
            },
            |ctx| {
                let delta = ctx.input(|input| input.zoom_delta());
                assert!((delta - 1.25).abs() < 0.001, "zoom delta = {delta}");
            },
        );
    }

    #[test]
    fn lsp_columns_use_utf16_units() {
        // The emoji occupies two UTF-16 code units but one Rust char.
        let text = "a😀b\nx";
        assert_eq!(char_index_for_lsp(text, 0, 0), 0);
        assert_eq!(char_index_for_lsp(text, 0, 1), 1);
        assert_eq!(char_index_for_lsp(text, 0, 3), 2);
        assert_eq!(char_index_for_lsp(text, 1, 0), 4);
    }
}
