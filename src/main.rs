mod app;
mod dialogs;
mod editor;
mod explorer;
mod highlight;
mod lsp;
mod terminal;
mod theme;

use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
mod macos_open {
    use std::ffi::{c_char, CStr};
    use std::path::PathBuf;
    use std::sync::Mutex;

    static OPEN_FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

    extern "C" fn on_open_file(path: *const c_char) {
        if path.is_null() {
            return;
        }
        let path = unsafe { CStr::from_ptr(path) }
            .to_string_lossy()
            .into_owned();
        if let Ok(mut files) = OPEN_FILES.lock() {
            files.push(PathBuf::from(path));
        }
    }

    extern "C" {
        fn kodmoz_install_open_handler(callback: extern "C" fn(*const c_char));
    }

    pub fn install() {
        unsafe { kodmoz_install_open_handler(on_open_file) };
    }

    pub fn take() -> Vec<PathBuf> {
        OPEN_FILES
            .lock()
            .map(|mut files| std::mem::take(&mut *files))
            .unwrap_or_default()
    }
}

#[cfg(not(target_os = "macos"))]
mod macos_open {
    use std::path::PathBuf;

    pub fn install() {}
    pub fn take() -> Vec<PathBuf> {
        Vec::new()
    }
}

fn main() -> eframe::Result<()> {
    macos_open::install();

    let (root, file) = resolve_args();

    #[cfg(debug_assertions)]
    eprintln!(
        "Kodmoz Code başlatılıyor: cwd={:?} kök={:?} dosya={:?}",
        std::env::current_dir(),
        root,
        file
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("Kodmoz Code"),
        ..Default::default()
    };

    eframe::run_native(
        "Kodmoz Code",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::KodmozCode::new(
                cc,
                root.clone(),
                file.clone(),
            )))
        }),
    )
}

/// Accepts `kodmoz-code [folder] [file]` or `kodmoz-code path/to/file.rs`.
/// A file argument resolves its workspace by walking up to Cargo.toml / .git.
fn resolve_args() -> (Option<PathBuf>, Option<PathBuf>) {
    let mut args = std::env::args().skip(1);
    let first = args.next().map(|arg| canonicalize(PathBuf::from(arg)));
    let second = args.next().map(|arg| canonicalize(PathBuf::from(arg)));

    match (first, second) {
        (None, _) => (None, None),
        (Some(first), Some(second)) if first.is_dir() => (Some(first), Some(second)),
        (Some(first), Some(second)) if first.is_file() => (Some(detect_root(&first)), Some(second)),
        (Some(path), None) if path.is_file() => (Some(detect_root(&path)), Some(path)),
        (Some(path), _) if path.is_dir() => (Some(path), None),
        (Some(path), _) => (path.parent().map(Path::to_path_buf), Some(path)),
    }
}

fn canonicalize(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn detect_root(file: &Path) -> PathBuf {
    let start = file.parent().unwrap_or(file);
    for ancestor in start.ancestors() {
        if ancestor.join("Cargo.toml").is_file() || ancestor.join(".git").exists() {
            return ancestor.to_path_buf();
        }
    }
    start.to_path_buf()
}
