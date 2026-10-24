use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};

use serde_json::{json, Value};

#[derive(Debug, Clone)]
#[allow(dead_code)] // end positions and source are kept for future inline diagnostics.
pub struct DiagnosticItem {
    pub severity: u8,
    pub message: String,
    /// Zero-based line.
    pub line: u32,
    /// Zero-based UTF-16 column, as defined by LSP.
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub source: String,
}

impl DiagnosticItem {
    pub fn is_error(&self) -> bool {
        self.severity == 1
    }

    pub fn is_warning(&self) -> bool {
        self.severity == 2
    }
}

pub enum LspEvent {
    Message(Value),
    Exited(String),
}

pub struct LspClient {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    rx: mpsc::Receiver<LspEvent>,
    next_id: i64,
}

impl LspClient {
    pub fn spawn(root: &Path, binary: &Path) -> std::io::Result<Self> {
        let mut command = Command::new(binary);
        command
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(path) = augmented_path() {
            command.env("PATH", path);
        }
        let mut child = command.spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("LSP stdin alınamadı"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("LSP stdout alınamadı"))?;

        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("kodmoz-lsp-reader".to_string())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    match read_message(&mut reader) {
                        Ok(Some(value)) => {
                            if tx.send(LspEvent::Message(value)).is_err() {
                                break;
                            }
                        }
                        Ok(None) => {
                            let _ =
                                tx.send(LspEvent::Exited("sunucu bağlantıyı kapattı".to_string()));
                            break;
                        }
                        Err(err) => {
                            let _ = tx.send(LspEvent::Exited(format!("okuma hatası: {err}")));
                            break;
                        }
                    }
                }
            })?;

        Ok(Self {
            child,
            stdin: Arc::new(Mutex::new(stdin)),
            rx,
            next_id: 1,
        })
    }

    pub fn request(&mut self, method: &str, params: Value) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        id
    }

    pub fn notify(&self, method: &str, params: Value) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    pub fn respond(&self, id: Value, result: Value) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }));
    }

    pub fn try_recv(&self) -> Option<LspEvent> {
        self.rx.try_recv().ok()
    }

    fn send(&self, message: &Value) {
        let body = match serde_json::to_vec(message) {
            Ok(body) => body,
            Err(err) => {
                eprintln!("LSP: mesaj serileştirilemedi: {err}");
                return;
            }
        };
        let mut stdin = match self.stdin.lock() {
            Ok(stdin) => stdin,
            Err(poisoned) => poisoned.into_inner(),
        };
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        if stdin.write_all(header.as_bytes()).is_err() {
            return;
        }
        let _ = stdin.write_all(&body);
        let _ = stdin.flush();
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_message(reader: &mut impl BufRead) -> std::io::Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let Some(length) = content_length else {
        return Ok(None);
    };
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let value = serde_json::from_slice(&body)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    Ok(Some(value))
}

/// PATH handed to child processes. Finder-launched apps get a minimal PATH,
/// which is not enough for rust-analyzer to find cargo/rustc.
pub fn augmented_path() -> Option<std::ffi::OsString> {
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        for relative in [".cargo/bin", ".local/bin"] {
            let dir = home.join(relative);
            if dir.is_dir() && !paths.contains(&dir) {
                paths.push(dir);
            }
        }
    }
    for dir in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        let dir = PathBuf::from(dir);
        if dir.is_dir() && !paths.contains(&dir) {
            paths.push(dir);
        }
    }
    std::env::join_paths(paths).ok()
}

pub fn path_to_uri(path: &Path) -> Option<String> {
    url::Url::from_file_path(path)
        .ok()
        .map(|url| url.to_string())
}

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}

/// Looks for rust-analyzer in common locations, including the rustup proxy.
pub fn find_rust_analyzer() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("RUST_ANALYZER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join("rust-analyzer");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        for relative in [".cargo/bin/rust-analyzer", ".local/bin/rust-analyzer"] {
            let candidate = home.join(relative);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    if let Ok(output) = Command::new("rustup")
        .args(["which", "rust-analyzer"])
        .output()
    {
        if output.status.success() {
            let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            if path.is_file() {
                return Some(path);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_content_length_framed_messages() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let framed = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut cursor = std::io::Cursor::new(framed.into_bytes());

        let value = read_message(&mut cursor).unwrap().unwrap();
        assert_eq!(value["id"], 1);
        assert!(read_message(&mut cursor).unwrap().is_none());
    }

    #[test]
    fn uri_roundtrip() {
        let path = PathBuf::from("/tmp/kodmoz-demo/src/main.rs");
        let uri = path_to_uri(&path).unwrap();
        assert!(uri.starts_with("file://"));
        assert_eq!(uri_to_path(&uri).unwrap(), path);
    }
}
