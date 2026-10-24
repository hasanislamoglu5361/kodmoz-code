use std::ops::Range;
use std::path::Path;

use egui::text::{LayoutJob, LayoutSection, TextFormat};
use egui::{Color32, FontId};
use streaming_iterator::StreamingIterator;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use tree_sitter::{Parser, Query, QueryCursor};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LanguageId {
    Rust,
    Plain,
    Image,
    Binary,
    Syntax { name: String, token: String },
}

impl LanguageId {
    pub fn from_path(path: &Path) -> Self {
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .unwrap_or_default();

        if matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tif" | "tiff"
        ) {
            return Self::Image;
        }

        if file_name == "dockerfile"
            || file_name.starts_with("dockerfile.")
            || extension == "dockerfile"
        {
            return syntax("Dockerfile", "dockerfile");
        }
        if matches!(file_name.as_str(), "makefile" | "gnumakefile")
            || matches!(extension.as_str(), "mk" | "mak" | "am")
        {
            return syntax("Makefile", "make");
        }
        if file_name == "cargo.lock" {
            return syntax("TOML", "toml");
        }
        if file_name == "cmakelists.txt" {
            return syntax("CMake", "cmake");
        }
        if file_name == "jenkinsfile" {
            return syntax("Groovy", "groovy");
        }
        if file_name == ".env" || file_name.starts_with(".env.") {
            return syntax("ENV", "ini");
        }

        let (name, token) = match extension.as_str() {
            "rs" => return Self::Rust,
            "sh" | "bash" | "zsh" | "fish" => ("Shell", "sh"),
            "php" | "phtml" => ("PHP", "php"),
            "swift" => ("Swift", "swift"),
            "yaml" | "yml" => ("YAML", "yaml"),
            "tf" | "tfvars" | "hcl" => ("Terraform", "tf"),
            "toml" => ("TOML", "toml"),
            "json" | "jsonc" | "json5" => ("JSON", "json"),
            "md" | "markdown" => ("Markdown", "md"),
            "js" | "mjs" | "cjs" => ("JavaScript", "js"),
            "jsx" => ("JavaScript React", "jsx"),
            "ts" | "mts" | "cts" => ("TypeScript", "ts"),
            "tsx" => ("TypeScript React", "tsx"),
            "py" | "pyw" => ("Python", "py"),
            "go" => ("Go", "go"),
            "c" | "h" => ("C", "c"),
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => ("C++", "cpp"),
            "cs" => ("C#", "cs"),
            "java" => ("Java", "java"),
            "kt" | "kts" => ("Kotlin", "kt"),
            "rb" | "rake" => ("Ruby", "rb"),
            "lua" => ("Lua", "lua"),
            "sql" => ("SQL", "sql"),
            "html" | "htm" => ("HTML", "html"),
            "css" => ("CSS", "css"),
            "scss" => ("SCSS", "scss"),
            "sass" => ("Sass", "sass"),
            "less" => ("Less", "less"),
            "xml" | "svg" => ("XML", "xml"),
            "ini" | "cfg" | "conf" => ("INI", "ini"),
            "vue" => ("Vue", "vue"),
            "pl" | "pm" => ("Perl", "pl"),
            "r" => ("R", "r"),
            "dart" => ("Dart", "dart"),
            "ex" | "exs" => ("Elixir", "ex"),
            "erl" | "hrl" => ("Erlang", "erl"),
            "hs" => ("Haskell", "hs"),
            "clj" | "cljs" | "cljc" => ("Clojure", "clj"),
            "scala" => ("Scala", "scala"),
            "groovy" | "gradle" => ("Groovy", "groovy"),
            "cmake" => ("CMake", "cmake"),
            "txt" | "text" | "log" | "lock" | "" => return Self::Plain,
            other => {
                let label = other.to_ascii_uppercase();
                return Self::Syntax {
                    name: label,
                    token: other.to_string(),
                };
            }
        };

        syntax(name, token)
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Rust => "Rust",
            Self::Plain => "Plain Text",
            Self::Image => "Görsel",
            Self::Binary => "İkili",
            Self::Syntax { name, .. } => name,
        }
    }

    pub fn lsp_id(&self) -> Option<&'static str> {
        match self {
            Self::Rust => Some("rust"),
            _ => None,
        }
    }

    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::Image | Self::Binary)
    }

    pub fn syntax_token(&self) -> Option<&str> {
        match self {
            Self::Syntax { token, .. } => Some(token),
            _ => None,
        }
    }
}

fn syntax(name: &str, token: &str) -> LanguageId {
    LanguageId::Syntax {
        name: name.to_string(),
        token: token.to_string(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenStyle {
    Default,
    Keyword,
    Control,
    Type,
    Function,
    Macro,
    Variable,
    Property,
    Constant,
    Number,
    String,
    Escape,
    Comment,
    Attribute,
    Label,
}

impl TokenStyle {
    fn color(self) -> Color32 {
        match self {
            Self::Default => Color32::from_rgb(0xD4, 0xD4, 0xD4),
            Self::Keyword => Color32::from_rgb(0x56, 0x9C, 0xD6),
            Self::Control => Color32::from_rgb(0xC5, 0x86, 0xC0),
            Self::Type => Color32::from_rgb(0x4E, 0xC9, 0xB0),
            Self::Function => Color32::from_rgb(0xDC, 0xDC, 0xAA),
            Self::Macro => Color32::from_rgb(0xDC, 0xDC, 0xAA),
            Self::Variable => Color32::from_rgb(0x9C, 0xDC, 0xFE),
            Self::Property => Color32::from_rgb(0x9C, 0xDC, 0xFE),
            Self::Constant => Color32::from_rgb(0x4F, 0xC1, 0xFF),
            Self::Number => Color32::from_rgb(0xB5, 0xCE, 0xA8),
            Self::String => Color32::from_rgb(0xCE, 0x91, 0x78),
            Self::Escape => Color32::from_rgb(0xD7, 0xBA, 0x7D),
            Self::Comment => Color32::from_rgb(0x6A, 0x99, 0x55),
            Self::Attribute => Color32::from_rgb(0xDC, 0xDC, 0xAA),
            Self::Label => Color32::from_rgb(0xC5, 0x86, 0xC0),
        }
    }

    /// Higher priority is applied later, so more specific captures win.
    fn priority(self) -> u8 {
        match self {
            Self::Default => 0,
            Self::Variable => 10,
            Self::Label => 12,
            Self::Property => 15,
            Self::Type => 20,
            Self::Function | Self::Macro => 25,
            Self::Keyword => 30,
            Self::Control => 31,
            Self::Number => 35,
            Self::Constant => 36,
            Self::Comment => 40,
            Self::String => 45,
            Self::Escape => 50,
            Self::Attribute => 50,
        }
    }
}

fn style_for_capture(name: &str) -> Option<TokenStyle> {
    if name == "keyword.control" {
        return Some(TokenStyle::Control);
    }
    if name.starts_with("keyword") {
        return Some(TokenStyle::Keyword);
    }
    if name == "string.escape" {
        return Some(TokenStyle::Escape);
    }
    if name.starts_with("string") {
        return Some(TokenStyle::String);
    }
    if name.starts_with("comment") {
        return Some(TokenStyle::Comment);
    }
    if name.starts_with("number") {
        return Some(TokenStyle::Number);
    }
    if name.starts_with("type") {
        return Some(TokenStyle::Type);
    }
    if name == "function.macro" {
        return Some(TokenStyle::Macro);
    }
    if name.starts_with("function") {
        return Some(TokenStyle::Function);
    }
    if name == "variable.builtin" {
        return Some(TokenStyle::Constant);
    }
    if name.starts_with("variable") {
        return Some(TokenStyle::Variable);
    }
    if name == "property" {
        return Some(TokenStyle::Property);
    }
    if name == "constant" {
        return Some(TokenStyle::Constant);
    }
    if name == "label" {
        return Some(TokenStyle::Label);
    }
    if name == "attribute" {
        return Some(TokenStyle::Attribute);
    }
    None
}

const RUST_HIGHLIGHT_QUERY: &str = r#"
(line_comment) @comment
(block_comment) @comment
(string_literal) @string
(raw_string_literal) @string
(char_literal) @string
(escape_sequence) @string.escape
(integer_literal) @number
(float_literal) @number
(boolean_literal) @constant
(type_identifier) @type
(primitive_type) @type.builtin
(field_identifier) @property
(lifetime) @label
(identifier) @variable
(function_item name: (identifier) @function)
(call_expression function: (identifier) @function.call)
(call_expression function: (field_expression field: (field_identifier) @function.method))
(macro_invocation macro: (identifier) @function.macro)
(attribute_item) @attribute
(mutable_specifier) @keyword.modifier
(crate) @keyword
(self) @keyword
(super) @keyword
[
  "as" "async" "await" "break" "const" "continue" "dyn" "else" "enum"
  "extern" "false" "fn" "for" "if" "impl" "in" "let" "loop" "match" "mod"
  "move" "pub" "ref" "return" "static" "struct" "trait"
  "true" "type" "unsafe" "use" "where" "while" "yield" "union" "default"
] @keyword
[
  "async" "await" "break" "continue" "else" "for" "if" "loop" "match"
  "return" "while"
] @keyword.control
"#;

pub struct Highlighter {
    parser: Parser,
    rust_query: Option<Query>,
    query_error: Option<String>,
    syntax_set: SyntaxSet,
    theme: Theme,
}

impl Highlighter {
    pub fn new() -> Self {
        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        if let Err(err) = parser.set_language(&language) {
            eprintln!("tree-sitter: Rust dil bilgisi yüklenemedi: {err}");
        }
        let (rust_query, query_error) = match Query::new(&language, RUST_HIGHLIGHT_QUERY) {
            Ok(query) => (Some(query), None),
            Err(err) => {
                eprintln!("tree-sitter: vurgulama sorgusu derlenemedi: {err}");
                (None, Some(err.to_string()))
            }
        };

        let syntax_set = two_face::syntax::extra_newlines();
        let mut theme_set = ThemeSet::load_defaults();
        let theme = theme_set
            .themes
            .remove("base16-eighties.dark")
            .or_else(|| theme_set.themes.into_iter().next().map(|(_, theme)| theme))
            .unwrap_or_else(|| Theme::default());

        Self {
            parser,
            rust_query,
            query_error,
            syntax_set,
            theme,
        }
    }

    /// Returns the source of the first query error, if highlighting is disabled.
    pub fn query_error(&self) -> Option<&str> {
        self.query_error.as_deref()
    }

    pub fn highlight(
        &mut self,
        text: &str,
        language: &LanguageId,
        font: FontId,
        wrap_width: f32,
    ) -> LayoutJob {
        if text.is_empty() {
            let mut job = LayoutJob {
                text: String::new(),
                ..Default::default()
            };
            job.wrap.max_width = wrap_width;
            return job;
        }

        if *language == LanguageId::Rust {
            if let Some(job) = self.highlight_rust(text, &font, wrap_width) {
                return job;
            }
        } else if let Some(token) = language.syntax_token() {
            if let Some(job) = self.highlight_syntect(text, token, &font, wrap_width) {
                return job;
            }
        }

        plain_job(text, &font, wrap_width)
    }

    fn highlight_rust(&mut self, text: &str, font: &FontId, wrap_width: f32) -> Option<LayoutJob> {
        let query = self.rust_query.as_ref()?;
        let tree = self.parser.parse(text, None)?;

        let mut styles = vec![TokenStyle::Default; text.len()];
        let mut cursor = QueryCursor::new();
        let mut spans: Vec<(u8, Range<usize>, TokenStyle)> = Vec::new();
        let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());
        while let Some(m) = matches.next() {
            for capture in m.captures {
                let name = query
                    .capture_names()
                    .get(capture.index as usize)
                    .copied()
                    .unwrap_or("");
                let Some(style) = style_for_capture(name) else {
                    continue;
                };
                let range = capture.node.byte_range();
                if range.start < range.end && range.end <= text.len() {
                    spans.push((style.priority(), range, style));
                }
            }
        }
        spans.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| (b.1.end - b.1.start).cmp(&(a.1.end - a.1.start)))
        });
        for (_, range, style) in spans {
            for slot in &mut styles[range] {
                *slot = style;
            }
        }

        let mut job = LayoutJob {
            text: text.to_owned(),
            ..Default::default()
        };
        job.wrap.max_width = wrap_width;
        job.break_on_newline = true;

        let mut start = 0;
        while start < text.len() {
            let style = styles[start];
            let mut end = start + 1;
            while end < text.len() && styles[end] == style {
                end += 1;
            }
            job.sections.push(LayoutSection {
                leading_space: 0.0,
                byte_range: start..end,
                format: TextFormat {
                    font_id: font.clone(),
                    color: style.color(),
                    ..Default::default()
                },
            });
            start = end;
        }
        Some(job)
    }

    fn highlight_syntect(
        &self,
        text: &str,
        token: &str,
        font: &FontId,
        wrap_width: f32,
    ) -> Option<LayoutJob> {
        let syntax = self
            .syntax_set
            .find_syntax_by_extension(token)
            .or_else(|| self.syntax_set.find_syntax_by_token(token))
            .or_else(|| self.syntax_set.find_syntax_by_name(token))?;

        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut job = LayoutJob {
            text: text.to_owned(),
            ..Default::default()
        };
        job.wrap.max_width = wrap_width;
        job.break_on_newline = true;

        let mut offset = 0usize;
        for line in LinesWithEndings::from(text) {
            let ranges = highlighter.highlight_line(line, &self.syntax_set).ok()?;
            for (style, piece) in ranges {
                if piece.is_empty() {
                    continue;
                }
                let start = offset;
                let end = offset + piece.len();
                offset = end;
                let color =
                    Color32::from_rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                let mut format = TextFormat {
                    font_id: font.clone(),
                    color,
                    ..Default::default()
                };
                if style.font_style.contains(FontStyle::ITALIC) {
                    format.italics = true;
                }
                if style.font_style.contains(FontStyle::UNDERLINE) {
                    format.underline = egui::Stroke::new(1.0, color);
                }
                job.sections.push(LayoutSection {
                    leading_space: 0.0,
                    byte_range: start..end,
                    format,
                });
            }
        }

        if offset != text.len() {
            return None;
        }
        Some(job)
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

fn plain_job(text: &str, font: &FontId, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob {
        text: text.to_owned(),
        ..Default::default()
    };
    job.wrap.max_width = wrap_width;
    job.break_on_newline = true;
    if !text.is_empty() {
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: 0..text.len(),
            format: TextFormat {
                font_id: font.clone(),
                color: TokenStyle::Default.color(),
                ..Default::default()
            },
        });
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_query_compiles() {
        let highlighter = Highlighter::new();
        assert!(
            highlighter.query_error().is_none(),
            "query error: {:?}",
            highlighter.query_error()
        );
    }

    #[test]
    fn highlights_rust_keywords_and_strings() {
        let mut highlighter = Highlighter::new();
        let source = "fn main() {\n    let x = \"hello\";\n    // done\n}\n";
        let job = highlighter.highlight(
            source,
            &LanguageId::Rust,
            FontId::monospace(14.0),
            f32::INFINITY,
        );
        assert_eq!(job.text, source);
        assert!(job.sections.len() > 3);
        let covered: usize = job.sections.iter().map(|s| s.byte_range.len()).sum();
        assert_eq!(covered, source.len());
        let keyword = job
            .sections
            .iter()
            .find(|s| &source[s.byte_range.clone()] == "fn")
            .expect("keyword section");
        assert_eq!(keyword.format.color, TokenStyle::Keyword.color());
        let string = job
            .sections
            .iter()
            .find(|s| &source[s.byte_range.clone()] == "\"hello\"")
            .expect("string section");
        assert_eq!(string.format.color, TokenStyle::String.color());
    }

    #[test]
    fn plain_text_is_single_section() {
        let mut highlighter = Highlighter::new();
        let job = highlighter.highlight(
            "hello world\nsecond line",
            &LanguageId::Plain,
            FontId::monospace(14.0),
            600.0,
        );
        assert_eq!(job.sections.len(), 1);
    }

    #[test]
    fn detects_language_and_read_only_kinds() {
        assert_eq!(
            LanguageId::from_path(Path::new("src/main.rs")),
            LanguageId::Rust
        );
        assert_eq!(
            LanguageId::from_path(Path::new("photo.PNG")),
            LanguageId::Image
        );
        assert_eq!(
            LanguageId::from_path(Path::new("notes.txt")),
            LanguageId::Plain
        );
        assert_eq!(
            LanguageId::from_path(Path::new("deploy.sh")),
            syntax("Shell", "sh")
        );
        assert_eq!(
            LanguageId::from_path(Path::new("Dockerfile")),
            syntax("Dockerfile", "dockerfile")
        );
        assert_eq!(
            LanguageId::from_path(Path::new("infra/main.tf")),
            syntax("Terraform", "tf")
        );
        assert!(LanguageId::Image.is_read_only());
        assert!(LanguageId::Binary.is_read_only());
        assert!(!LanguageId::Rust.is_read_only());
    }

    #[test]
    fn highlights_other_languages_with_syntect() {
        let mut highlighter = Highlighter::new();
        let source =
            "services:\n  web:\n    image: nginx:latest\n    ports:\n      - \"8080:80\"\n";
        let job = highlighter.highlight(
            source,
            &syntax("YAML", "yaml"),
            FontId::monospace(14.0),
            f32::INFINITY,
        );
        assert_eq!(job.text, source);
        assert!(job.sections.len() > 3);
        let covered: usize = job.sections.iter().map(|s| s.byte_range.len()).sum();
        assert_eq!(covered, source.len());
    }
}
