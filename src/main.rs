mod app;
mod dialogs;
mod editor;
mod explorer;
mod highlight;
mod lsp;
mod theme;

use std::path::{Path, PathBuf};

fn main() -> eframe::Result<()> {
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
