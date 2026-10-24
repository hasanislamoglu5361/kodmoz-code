use std::ops::Range;
use std::path::Path;

use egui::text::{LayoutJob, LayoutSection, TextFormat};
use egui::{Color32, FontId};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LanguageId {
    Rust,
    Plain,
    Image,
    Binary,
}

impl LanguageId {
    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref()
        {
            Some("rs") => Self::Rust,
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tif" | "tiff") => {
                Self::Image
            }
            _ => Self::Plain,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Plain => "Plain Text",
            Self::Image => "Görsel",
            Self::Binary => "İkili",
        }
    }

    pub fn lsp_id(self) -> Option<&'static str> {
        match self {
            Self::Rust => Some("rust"),
            _ => None,
        }
    }

    pub fn is_read_only(self) -> bool {
        matches!(self, Self::Image | Self::Binary)
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
        Self {
            parser,
            rust_query,
            query_error,
        }
    }

    /// Returns the source of the first query error, if highlighting is disabled.
    pub fn query_error(&self) -> Option<&str> {
        self.query_error.as_deref()
    }

    pub fn highlight(
        &mut self,
        text: &str,
        language: LanguageId,
        font: FontId,
        wrap_width: f32,
    ) -> LayoutJob {
        let mut job = LayoutJob {
            text: text.to_owned(),
            ..Default::default()
        };
        job.wrap.max_width = wrap_width;
        job.break_on_newline = true;

        if text.is_empty() {
            return job;
        }

        let mut styles = vec![TokenStyle::Default; text.len()];
        if language == LanguageId::Rust {
            if let Some(query) = &self.rust_query {
                if let Some(tree) = self.parser.parse(text, None) {
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
                }
            }
        }

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
        job
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
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
            LanguageId::Rust,
            FontId::monospace(14.0),
            f32::INFINITY,
        );
        assert_eq!(job.text, source);
        assert!(job.sections.len() > 3);
        // Every byte of the input must be covered by exactly one section.
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
            LanguageId::Plain,
            FontId::monospace(14.0),
            600.0,
        );
        assert_eq!(job.sections.len(), 1);
    }

    #[test]
    fn detects_language_and_read_only_kinds() {
        assert_eq!(
            LanguageId::from_path(std::path::Path::new("src/main.rs")),
            LanguageId::Rust
        );
        assert_eq!(
            LanguageId::from_path(std::path::Path::new("photo.PNG")),
            LanguageId::Image
        );
        assert_eq!(
            LanguageId::from_path(std::path::Path::new("notes.txt")),
            LanguageId::Plain
        );
        assert!(LanguageId::Image.is_read_only());
        assert!(LanguageId::Binary.is_read_only());
        assert!(!LanguageId::Rust.is_read_only());
    }
}
