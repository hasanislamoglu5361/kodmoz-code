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

    let (root, files) = resolve_args();

    #[cfg(debug_assertions)]
    eprintln!(
        "Kodmoz Code başlatılıyor: cwd={:?} kök={:?} dosyalar={:?}",
        std::env::current_dir(),
        root,
        files
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
                files.clone(),
            )))
        }),
    )
}

/// Accepts `kodmoz-code [folder] [file...]` or `kodmoz-code file...`.
/// File arguments resolve the workspace by walking up to Cargo.toml / .git.
fn resolve_args() -> (Option<PathBuf>, Vec<PathBuf>) {
    let args: Vec<PathBuf> = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .map(canonicalize)
        .collect();
    let Some(first) = args.first() else {
        return (None, Vec::new());
    };

    if first.is_dir() {
        let root = first.clone();
        let files = args
            .iter()
            .skip(1)
            .map(|path| {
                let path = if path.is_absolute() {
                    path.clone()
                } else {
                    root.join(path)
                };
                canonicalize(path)
            })
            .collect();
        (Some(root), files)
    } else {
        (Some(detect_root(first)), args)
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
