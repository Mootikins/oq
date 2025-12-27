//! oq - Object Query
//!
//! A jq-like tool for querying JSON, YAML, TOML, and TOON data.
//!
//! # Example
//!
//! ```rust
//! use oq::{compile_filter, run_filter, parse_auto, encode_to_format, Format};
//! use serde_json::json;
//!
//! // Basic query
//! let filter = compile_filter(".name").unwrap();
//! let input = json!({"name": "Ada", "age": 30});
//! let results = run_filter(&filter, input).unwrap();
//! assert_eq!(results[0], json!("Ada"));
//!
//! // Parse any format (auto-detected)
//! let yaml_input = "name: Ada\nage: 30";
//! let value = parse_auto(yaml_input).unwrap();
//!
//! // Convert between formats
//! let toml_output = encode_to_format(&value, Format::Toml).unwrap();
//! ```

mod convert;
mod formatter;
mod mapper;
mod query;
mod tabular;

pub use convert::{
    detect_format, encode_to_format, parse_auto, parse_input, to_json, to_toml, to_toon, to_yaml,
    Format, InputFormat, OutputFormat,
};
pub use formatter::{
    command_formatter, read_note_formatter, search_formatter, ContentFormatter, FieldFormat,
};
pub use mapper::{
    default_registry, ChainMapper, FieldSelectMapper, IdentityMapper, JqMapper, LimitMapper,
    Mapper, MapperRegistry, TruncateMapper,
};
pub use query::{compile_filter, run_filter, CompiledFilter};
pub use tabular::encode_table;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum OqError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(String),

    #[error("TOON parse error: {0}")]
    ToonParse(String),

    #[error("Filter error: {0}")]
    Filter(String),

    #[error("Query compile error: {0}")]
    Compile(String),
}

/// Convert a JSON value to TOON string
pub fn json_to_toon(value: serde_json::Value) -> Result<String, OqError> {
    toon_format::encode_default(&value).map_err(|e| OqError::ToonParse(e.to_string()))
}

/// Convert a JSON value to TOON string, applying a mapper first
pub fn json_to_toon_with_mapper(
    value: serde_json::Value,
    tool_name: &str,
    registry: &MapperRegistry,
) -> Result<String, OqError> {
    let transformed = registry.transform(tool_name, value)?;
    json_to_toon(transformed)
}

// ============================================================================
// High-level API for MCP tool response formatting
// ============================================================================

/// Format a JSON value as TOON, with fallback to JSON string on error
pub fn format_tool_response(value: &serde_json::Value) -> String {
    toon_format::encode_default(value).unwrap_or_else(|_| {
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    })
}

/// Format a JSON value as TOON with smart content extraction
pub fn format_tool_response_smart(value: &serde_json::Value) -> String {
    let formatter = ContentFormatter::new().with_default_threshold(200);
    formatter
        .format(value)
        .unwrap_or_else(|_| format_tool_response(value))
}

/// Format a tool response with a specific formatter type
pub fn format_tool_response_with(value: &serde_json::Value, tool_type: ToolType) -> String {
    let formatter = match tool_type {
        ToolType::ReadFile => read_note_formatter(),
        ToolType::Search => search_formatter(),
        ToolType::Command => command_formatter(),
        ToolType::Generic => ContentFormatter::new().with_default_threshold(200),
    };
    formatter
        .format(value)
        .unwrap_or_else(|_| format_tool_response(value))
}

/// Tool types for smart formatting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolType {
    /// File read operations (content field extracted)
    ReadFile,
    /// Search operations (snippets extracted)
    Search,
    /// Shell/command output (stdout/stderr extracted)
    Command,
    /// Generic tool (auto-detect long fields)
    Generic,
}

impl ToolType {
    /// Infer tool type from tool name
    pub fn from_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();

        if name_lower.contains("search")
            || name_lower.contains("find")
            || name_lower.contains("grep")
        {
            ToolType::Search
        } else if name_lower.contains("exec")
            || name_lower.contains("run")
            || name_lower.contains("shell")
            || name_lower.contains("command")
        {
            ToolType::Command
        } else if name_lower.contains("read")
            || name_lower.contains("file")
            || (name_lower.contains("note") && name_lower.contains("get"))
            || (name_lower.contains("note") && name_lower.contains("content"))
        {
            ToolType::ReadFile
        } else {
            ToolType::Generic
        }
    }
}

/// Try to parse a string as JSON, returning the parsed value or the original string
pub fn try_parse_json(input: &str) -> serde_json::Value {
    serde_json::from_str(input).unwrap_or_else(|_| serde_json::Value::String(input.to_string()))
}

/// Format any string content - parses as JSON if possible, then formats as TOON
pub fn format_content(input: &str) -> String {
    let value = try_parse_json(input);
    format_tool_response(&value)
}

/// Format any string content with smart extraction
pub fn format_content_smart(input: &str) -> String {
    let value = try_parse_json(input);
    format_tool_response_smart(&value)
}
