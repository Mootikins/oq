//! Smart formatters for tool responses
//!
//! Since TOON doesn't support multiline strings natively, these formatters
//! provide intelligent handling for long content like file contents:
//!
//! 1. Extract long string fields into separate labeled blocks
//! 2. Use TOON for structured metadata
//! 3. Render content in readable formats (fenced code blocks, etc.)
//!
//! # Example Output
//!
//! ```text
//! path: src/main.rs
//! total_lines: 150
//! lines_returned: 50
//!
//! --- content ---
//! fn main() {
//!     println!("Hello, world!");
//! }
//! ```

use crate::OqError;
use serde_json::Value;
use std::collections::HashMap;

/// Configuration for how to format specific fields
#[derive(Debug, Clone)]
pub struct FieldFormat {
    /// Threshold in chars before extracting to block
    pub extract_threshold: usize,
    /// Label to use for extracted block (e.g., "content", "output")
    pub block_label: String,
    /// Whether to use fenced code block with language hint
    pub code_fence: Option<String>,
    /// Whether to truncate and add ellipsis
    pub max_length: Option<usize>,
}

impl Default for FieldFormat {
    fn default() -> Self {
        Self {
            extract_threshold: 200,
            block_label: "content".to_string(),
            code_fence: None,
            max_length: None,
        }
    }
}

impl FieldFormat {
    pub fn new(label: &str) -> Self {
        Self {
            block_label: label.to_string(),
            ..Default::default()
        }
    }

    pub fn with_threshold(mut self, threshold: usize) -> Self {
        self.extract_threshold = threshold;
        self
    }

    pub fn with_code_fence(mut self, language: &str) -> Self {
        self.code_fence = Some(language.to_string());
        self
    }

    pub fn with_max_length(mut self, max: usize) -> Self {
        self.max_length = Some(max);
        self
    }
}

/// Smart formatter that extracts long content into readable blocks
pub struct ContentFormatter {
    /// Field-specific formatting rules
    field_formats: HashMap<String, FieldFormat>,
    /// Default threshold for extracting fields
    default_threshold: usize,
    /// Whether to infer code language from file extension
    infer_language: bool,
}

impl Default for ContentFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentFormatter {
    pub fn new() -> Self {
        Self {
            field_formats: HashMap::new(),
            default_threshold: 200,
            infer_language: true,
        }
    }

    /// Set default extraction threshold
    pub fn with_default_threshold(mut self, threshold: usize) -> Self {
        self.default_threshold = threshold;
        self
    }

    /// Add formatting rules for a specific field
    pub fn with_field(mut self, field: &str, format: FieldFormat) -> Self {
        self.field_formats.insert(field.to_string(), format);
        self
    }

    /// Enable/disable language inference from path fields
    pub fn with_language_inference(mut self, enabled: bool) -> Self {
        self.infer_language = enabled;
        self
    }

    /// Format a JSON value, extracting long content into blocks
    pub fn format(&self, value: &Value) -> Result<String, OqError> {
        match value {
            Value::Object(map) => self.format_object(map),
            Value::Array(arr) => self.format_array(arr),
            other => self.format_simple(other),
        }
    }

    fn format_object(&self, map: &serde_json::Map<String, Value>) -> Result<String, OqError> {
        let mut metadata: serde_json::Map<String, Value> = serde_json::Map::new();
        let mut extracted_blocks: Vec<(String, String, Option<String>)> = Vec::new();

        // Detect file extension for language hint
        let language_hint = if self.infer_language {
            self.detect_language(map)
        } else {
            None
        };

        for (key, value) in map {
            let format = self.field_formats.get(key);
            let threshold = format
                .map(|f| f.extract_threshold)
                .unwrap_or(self.default_threshold);

            if let Value::String(s) = value {
                if s.len() > threshold || s.contains('\n') {
                    // Extract this field into a block
                    let label = format
                        .map(|f| f.block_label.clone())
                        .unwrap_or_else(|| key.clone());

                    let lang = format
                        .and_then(|f| f.code_fence.clone())
                        .or_else(|| language_hint.clone());

                    let content = if let Some(max) = format.and_then(|f| f.max_length) {
                        truncate_content(s, max)
                    } else {
                        s.clone()
                    };

                    extracted_blocks.push((label, content, lang));
                } else {
                    metadata.insert(key.clone(), value.clone());
                }
            } else {
                metadata.insert(key.clone(), value.clone());
            }
        }

        let mut output = String::new();

        // Render metadata as TOON
        if !metadata.is_empty() {
            let toon = toon_format::encode_default(&Value::Object(metadata))
                .map_err(|e| OqError::ToonParse(e.to_string()))?;
            output.push_str(&toon);
        }

        // Render extracted blocks
        for (label, content, lang) in extracted_blocks {
            if !output.is_empty() {
                output.push_str("\n\n");
            }

            if let Some(language) = lang {
                // Fenced code block
                output.push_str(&format!("--- {} ---\n```{}\n", label, language));
                output.push_str(&content);
                if !content.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str("```");
            } else {
                // Plain block
                output.push_str(&format!("--- {} ---\n", label));
                output.push_str(&content);
            }
        }

        Ok(output)
    }

    fn format_array(&self, arr: &[Value]) -> Result<String, OqError> {
        // For arrays, format each item
        let mut results = Vec::new();
        for (i, item) in arr.iter().enumerate() {
            let formatted = self.format(item)?;
            if arr.len() > 1 {
                results.push(format!("## Item {}\n{}", i + 1, formatted));
            } else {
                results.push(formatted);
            }
        }
        Ok(results.join("\n\n"))
    }

    fn format_simple(&self, value: &Value) -> Result<String, OqError> {
        toon_format::encode_default(value).map_err(|e| OqError::ToonParse(e.to_string()))
    }

    fn detect_language(&self, map: &serde_json::Map<String, Value>) -> Option<String> {
        // Look for path-like fields
        for key in &["path", "file", "filename", "file_path"] {
            if let Some(Value::String(path)) = map.get(*key) {
                return extension_to_language(path);
            }
        }
        None
    }
}

/// Truncate content with ellipsis
fn truncate_content(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Try to truncate at a line boundary
        let truncated: String = s.chars().take(max - 20).collect();
        if let Some(last_newline) = truncated.rfind('\n') {
            format!(
                "{}\n... ({} more characters)",
                &truncated[..last_newline],
                s.len() - last_newline
            )
        } else {
            format!("{}... ({} more characters)", truncated, s.len() - max + 20)
        }
    }
}

/// Map file extension to language identifier
fn extension_to_language(path: &str) -> Option<String> {
    let ext = path.rsplit('.').next()?;
    let lang = match ext.to_lowercase().as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "tsx" => "typescript",
        "jsx" => "javascript",
        "go" => "go",
        "rb" => "ruby",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "php" => "php",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" => "bash",
        "zsh" => "zsh",
        "fish" => "fish",
        "ps1" => "powershell",
        "sql" => "sql",
        "md" => "markdown",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "lua" => "lua",
        "r" => "r",
        "ex" | "exs" => "elixir",
        "erl" | "hrl" => "erlang",
        "hs" => "haskell",
        "ml" | "mli" => "ocaml",
        "clj" | "cljs" => "clojure",
        "lisp" | "cl" => "lisp",
        "vim" => "vim",
        "dockerfile" => "dockerfile",
        "tf" => "terraform",
        "proto" => "protobuf",
        "graphql" | "gql" => "graphql",
        _ => return None,
    };
    Some(lang.to_string())
}

/// Create a formatter configured for read_note tool responses
pub fn read_note_formatter() -> ContentFormatter {
    ContentFormatter::new()
        .with_default_threshold(100)
        .with_field(
            "content",
            FieldFormat::new("content")
                .with_threshold(100)
                .with_max_length(5000),
        )
}

/// Create a formatter configured for search tool responses
pub fn search_formatter() -> ContentFormatter {
    ContentFormatter::new()
        .with_default_threshold(300)
        .with_field("snippet", FieldFormat::new("match").with_threshold(200))
        .with_field("context", FieldFormat::new("context").with_threshold(200))
}

/// Create a formatter configured for command/shell output
pub fn command_formatter() -> ContentFormatter {
    ContentFormatter::new()
        .with_default_threshold(100)
        .with_field(
            "stdout",
            FieldFormat::new("stdout")
                .with_threshold(50)
                .with_code_fence(""),
        )
        .with_field(
            "stderr",
            FieldFormat::new("stderr")
                .with_threshold(50)
                .with_code_fence(""),
        )
        .with_field(
            "output",
            FieldFormat::new("output")
                .with_threshold(50)
                .with_code_fence(""),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_short_content_stays_inline() {
        let formatter = ContentFormatter::new().with_default_threshold(100);
        let value = json!({
            "path": "test.txt",
            "content": "Hello world"
        });
        let result = formatter.format(&value).unwrap();
        assert!(result.contains("path: test.txt"));
        assert!(result.contains("content: Hello world"));
        assert!(!result.contains("---"));
    }

    #[test]
    fn test_long_content_extracted() {
        let formatter = ContentFormatter::new().with_default_threshold(20);
        let value = json!({
            "path": "test.rs",
            "content": "fn main() {\n    println!(\"Hello, world!\");\n}"
        });
        let result = formatter.format(&value).unwrap();
        assert!(result.contains("path: test.rs"));
        assert!(result.contains("--- content ---"));
        assert!(result.contains("```rust"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn test_language_detection() {
        let formatter = ContentFormatter::new().with_default_threshold(10);

        let rust_file = json!({"path": "main.rs", "content": "fn main() {}"});
        let result = formatter.format(&rust_file).unwrap();
        assert!(result.contains("```rust"));

        let python_file = json!({"path": "script.py", "content": "def main(): pass"});
        let result = formatter.format(&python_file).unwrap();
        assert!(result.contains("```python"));
    }

    #[test]
    fn test_multiline_content_extracted() {
        let formatter = ContentFormatter::new().with_default_threshold(1000);
        let value = json!({
            "path": "test.txt",
            "content": "line1\nline2\nline3"
        });
        let result = formatter.format(&value).unwrap();
        // Multiline content should be extracted even if under threshold
        assert!(result.contains("--- content ---"));
    }

    #[test]
    fn test_truncation() {
        let formatter = ContentFormatter::new().with_field(
            "content",
            FieldFormat::new("content")
                .with_threshold(10)
                .with_max_length(50),
        );
        let long_content = "a".repeat(200);
        let value = json!({
            "path": "test.txt",
            "content": long_content
        });
        let result = formatter.format(&value).unwrap();
        assert!(result.contains("more characters"));
    }

    #[test]
    fn test_read_note_formatter() {
        let formatter = read_note_formatter();
        let value = json!({
            "path": "src/lib.rs",
            "content": "//! Library documentation\n\npub fn hello() {\n    println!(\"Hello!\");\n}",
            "total_lines": 5,
            "lines_returned": 5
        });
        let result = formatter.format(&value).unwrap();
        assert!(result.contains("path: src/lib.rs"));
        assert!(result.contains("total_lines: 5"));
        assert!(result.contains("--- content ---"));
        assert!(result.contains("```rust"));
    }
}
