use std::collections::HashMap;
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
use crate::theme;

pub(crate) struct Tab {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub name: String,
    pub content: String,
    pub dirty: bool,
    pub saved_hash: u64,
    pub language: LanguageId,
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
    pub(crate) diagnostics: HashMap<PathBuf, Vec<DiagnosticItem>>,
    pub(crate) quick_open: Option<QuickOpen>,
    pub(crate) palette: Option<Palette>,
    pub(crate) confirm_close: Option<usize>,
    pub(crate) status_msg: Option<(String, Instant)>,
    pub(crate) git_branch: Option<String>,
    pending_command: Option<CommandId>,
    last_title: String,
}

impl KodmozCode {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        root: Option<PathBuf>,
        file: Option<PathBuf>,
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
            font_size: 14.0,
            sidebar_visible: true,
            diagnostics_visible: std::env::var_os("KODMOZ_PROBLEMS").is_some(),
            lsp: None,
            lsp_ready: false,
            lsp_init_id: None,
            lsp_status: "LSP kapalı".to_string(),
            lsp_tried: false,
            diagnostics: HashMap::new(),
            quick_open: None,
            palette: None,
            confirm_close: None,
            status_msg: highlight_error.map(|message| (message, Instant::now())),
            git_branch: None,
            pending_command: None,
            last_title: String::new(),
        };

        if let Some(root) = root {
            app.open_folder(root);
        }
        if let Some(file) = file {
            app.open_file(file);
        }

        app
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status_msg = Some((message.into(), Instant::now()));
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
        self.lsp_status = "LSP bekliyor".to_string();
        self.diagnostics.clear();
        self.tabs.clear();
        self.active = None;
        self.quick_open = None;
        self.palette = None;
        self.confirm_close = None;
        self.root = Some(path.clone());
        self.explorer = Some(Explorer::new(path.clone()));
        self.git_branch = read_git_branch(&path);
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
            self.active = Some(index);
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
            language,
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
        self.active = Some(index);

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
        self.active = Some(self.tabs.len() - 1);
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
        if self.tabs.is_empty() {
            self.active = None;
        } else {
            let new_active = match self.active {
                Some(active) if active > index => Some(active - 1),
                Some(active) if active == index => Some(index.min(self.tabs.len() - 1)),
                Some(active) => Some(active),
                None => Some(0),
            };
            self.active = new_active;
        }
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
            tab.language = language;
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
            self.lsp_status = "Cargo projesi değil".to_string();
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
            ToggleDiagnostics,
            ZoomIn,
            ZoomOut,
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
            Some(Action::ToggleDiagnostics) => self.diagnostics_visible = !self.diagnostics_visible,
            Some(Action::ZoomIn) => {
                let size = (self.font_size + 1.0).min(28.0);
                self.set_font_size(ctx, size);
            }
            Some(Action::ZoomOut) => {
                let size = (self.font_size - 1.0).max(9.0);
                self.set_font_size(ctx, size);
            }
            None => {}
        }
    }

    fn set_font_size(&mut self, ctx: &egui::Context, size: f32) {
        self.font_size = size;
        ctx.style_mut(|style| {
            style
                .text_styles
                .insert(TextStyle::Monospace, FontId::monospace(size));
        });
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
            CommandId::ToggleSidebar => self.sidebar_visible = !self.sidebar_visible,
            CommandId::ToggleDiagnostics => self.diagnostics_visible = !self.diagnostics_visible,
            CommandId::ZoomIn => {
                let size = (self.font_size + 1.0).min(28.0);
                self.set_font_size(ctx, size);
            }
            CommandId::ZoomOut => {
                let size = (self.font_size - 1.0).max(9.0);
                self.set_font_size(ctx, size);
            }
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
                    let (mark, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                    theme::paint_brand_mark(ui.painter(), mark);
                    ui.add_space(3.0);
                    ui.label(
                        RichText::new("Kodmoz")
                            .strong()
                            .size(15.0)
                            .color(theme::TEXT),
                    );
                    ui.label(RichText::new("Code").size(12.5).color(theme::ACCENT));
                    ui.add_space(4.0);
                    ui.separator();

                    if ghost_button(ui, "Klasör Aç", "Cmd+O").clicked() {
                        self.pending_command = Some(CommandId::OpenFolder);
                    }
                    if ghost_button(ui, "Kaydet", "Cmd+S").clicked() {
                        self.pending_command = Some(CommandId::Save);
                    }
                    if ghost_button(ui, "Dosyaya Git", "Cmd+P").clicked() {
                        self.pending_command = Some(CommandId::QuickOpen);
                    }
                    if ghost_button(ui, "Komutlar", "Cmd+Shift+P").clicked() {
                        self.pending_command = Some(CommandId::Palette);
                    }
                    if ghost_button(ui, "Sorunlar", "Cmd+Shift+M").clicked() {
                        self.pending_command = Some(CommandId::ToggleDiagnostics);
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
                        }
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(&self.lsp_status)
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
                    ui.label(
                        RichText::new("EXPLORER")
                            .size(9.5)
                            .strong()
                            .color(theme::FAINT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ghost_button(ui, "Yenile", "").clicked() {
                            refresh = true;
                        }
                    });
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
                    if ghost_button(ui, "Klasör Aç", "").clicked() {
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
                            if diagnostic_row(ui, &text, severity_color(item.severity)).clicked() {
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
                        ui.label(
                            RichText::new(format!("git:{branch}"))
                                .size(9.5)
                                .color(theme::FAINT),
                        );
                    }
                    if is_read_only {
                        ui.add_space(8.0);
                        ui.label(RichText::new("salt okunur").size(9.0).color(theme::FAINT));
                    }
                    if errors > 0 {
                        ui.add_space(8.0);
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
                    self.editor_ui(ui);
                }
            });
    }

    fn tabs_bar(&mut self, ui: &mut Ui) {
        let infos: Vec<(usize, u64, String, bool, bool)> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                (
                    index,
                    tab.id,
                    tab.name.clone(),
                    tab.dirty,
                    Some(index) == self.active,
                )
            })
            .collect();
        let mut activate = None;
        let mut close = None;
        let font = FontId::proportional(12.0);

        let bar = egui::Frame::NONE
            .fill(theme::TABBAR_BG)
            .inner_margin(Margin {
                left: 6,
                right: 6,
                top: 5,
                bottom: 0,
            })
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("kodmoz_tabs")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (index, tab_id, name, dirty, selected) in &infos {
                                let text_width = ui.fonts(|fonts| {
                                    fonts
                                        .layout_no_wrap(name.clone(), font.clone(), theme::TEXT)
                                        .size()
                                        .x
                                });
                                let width = (text_width + 46.0).clamp(96.0, 260.0);
                                let (rect, response) =
                                    ui.allocate_exact_size(Vec2::new(width, 31.0), Sense::click());
                                let hovered = response.hovered();
                                let painter = ui.painter().with_clip_rect(rect);

                                if *selected {
                                    painter.rect_filled(
                                        rect,
                                        egui::CornerRadius {
                                            nw: 7,
                                            ne: 7,
                                            sw: 0,
                                            se: 0,
                                        },
                                        theme::BG,
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
                                } else if hovered {
                                    painter.rect_filled(
                                        rect,
                                        egui::CornerRadius {
                                            nw: 6,
                                            ne: 6,
                                            sw: 0,
                                            se: 0,
                                        },
                                        theme::SURFACE,
                                    );
                                }

                                painter.text(
                                    egui::pos2(rect.left() + 13.0, rect.center().y),
                                    Align2::LEFT_CENTER,
                                    name,
                                    font.clone(),
                                    if *selected { theme::TEXT } else { theme::MUTED },
                                );

                                let close_rect = egui::Rect::from_center_size(
                                    egui::pos2(rect.right() - 15.0, rect.center().y),
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
                                        close_rect.left() - 8.0
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
                        });
                    });
            });

        let rect = bar.response.rect;
        ui.painter().line_segment(
            [
                egui::pos2(rect.left(), rect.bottom()),
                egui::pos2(rect.right(), rect.bottom()),
            ],
            Stroke::new(1.0, theme::BORDER),
        );

        if let Some(index) = activate {
            self.active = Some(index);
        }
        if let Some(index) = close {
            self.request_close_tab(index);
        }
    }

    fn welcome(&mut self, ui: &mut Ui) {
        theme::paint_dot_grid(ui);
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.14).max(36.0));
            let (mark, _) = ui.allocate_exact_size(Vec2::splat(58.0), Sense::hover());
            theme::paint_brand_mark(ui.painter(), mark);
            ui.add_space(16.0);
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

            let cards_width = 3.0 * 178.0 + 2.0 * 10.0;
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - cards_width) * 0.5).max(0.0));
                welcome_card(ui, "Klasör Aç", "Cmd+O");
                welcome_card(ui, "Dosyaya Git", "Cmd+P");
                welcome_card(ui, "Komut Paleti", "Cmd+Shift+P");
            });
            ui.add_space(20.0);
            ui.label(
                RichText::new("Rust dosyası açtığında rust-analyzer otomatik başlar")
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
        self.handle_shortcuts(ctx);
        self.handle_dropped_files(ctx);
        self.poll_lsp(ctx);
        self.flush_lsp_changes(ctx);

        self.top_bar(ctx);
        self.status_bar(ctx);
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
    }
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

fn ghost_button(ui: &mut Ui, label: &str, shortcut: &str) -> egui::Response {
    let response = ui.add(
        egui::Button::new(RichText::new(label).size(11.5).color(theme::TEXT))
            .min_size(Vec2::new(0.0, 24.0)),
    );
    if shortcut.is_empty() {
        response
    } else {
        response.on_hover_text(shortcut)
    }
}

fn welcome_card(ui: &mut Ui, title: &str, shortcut: &str) {
    egui::Frame::NONE
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(150.0);
            ui.label(RichText::new(title).size(11.5).color(theme::TEXT));
            ui.add_space(10.0);
            theme::key_chip(ui, shortcut);
        });
}

fn diagnostic_row(ui: &mut Ui, text: &str, color: Color32) -> egui::Response {
    let font = FontId::proportional(11.0);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 21.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), theme::SURFACE);
    }
    let painter = ui.painter().with_clip_rect(rect);
    painter.circle_filled(egui::pos2(rect.left() + 8.0, rect.center().y), 2.5, color);
    let galley = ui.fonts(|fonts| fonts.layout_no_wrap(text.to_owned(), font, color));
    let text_pos = egui::pos2(rect.left() + 18.0, rect.center().y - galley.size().y * 0.5);
    painter.galley(text_pos, galley, color);
    response
}

fn status_separator(ui: &mut Ui) {
    ui.label(RichText::new("·").size(9.0).color(theme::FAINT));
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

fn severity_color(severity: u8) -> Color32 {
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
    fn lsp_columns_use_utf16_units() {
        // The emoji occupies two UTF-16 code units but one Rust char.
        let text = "a😀b\nx";
        assert_eq!(char_index_for_lsp(text, 0, 0), 0);
        assert_eq!(char_index_for_lsp(text, 0, 1), 1);
        assert_eq!(char_index_for_lsp(text, 0, 3), 2);
        assert_eq!(char_index_for_lsp(text, 1, 0), 4);
    }
}
