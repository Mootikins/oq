//! Format conversion between JSON, YAML, TOML, and TOON

use crate::OqError;
use serde_json::Value;

/// Supported input formats
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum InputFormat {
    /// Auto-detect based on content
    #[default]
    Auto,
    /// JSON format
    Json,
    /// YAML format
    Yaml,
    /// TOML format
    Toml,
    /// TOON format
    Toon,
}

/// Internal format representation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Yaml,
    Toml,
    Toon,
}

impl Format {
    /// Get format name as string
    pub fn name(&self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Yaml => "yaml",
            Format::Toml => "toml",
            Format::Toon => "toon",
        }
    }

    /// Parse format from string name
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "json" => Some(Format::Json),
            "yaml" | "yml" => Some(Format::Yaml),
            "toml" => Some(Format::Toml),
            "toon" => Some(Format::Toon),
            _ => None,
        }
    }
}

/// Output format options
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// JSON format
    Json,
    /// YAML format
    Yaml,
    /// TOML format
    Toml,
    /// TOON format (default)
    #[default]
    Toon,
}

impl From<OutputFormat> for Format {
    fn from(f: OutputFormat) -> Self {
        match f {
            OutputFormat::Json => Format::Json,
            OutputFormat::Yaml => Format::Yaml,
            OutputFormat::Toml => Format::Toml,
            OutputFormat::Toon => Format::Toon,
        }
    }
}

impl InputFormat {
    /// Detect the actual format from input content
    pub fn detect(&self, input: &str) -> Format {
        match self {
            InputFormat::Json => Format::Json,
            InputFormat::Yaml => Format::Yaml,
            InputFormat::Toml => Format::Toml,
            InputFormat::Toon => Format::Toon,
            InputFormat::Auto => detect_format(input),
        }
    }
}

/// Auto-detect format based on content heuristics
pub fn detect_format(input: &str) -> Format {
    let trimmed = input.trim();
    let lines: Vec<&str> = trimmed.lines().collect();

    // Empty input defaults to JSON
    if trimmed.is_empty() {
        return Format::Json;
    }

    // Check for TOML section headers first (looks like JSON array but isn't)
    let first_line = lines.first().map(|l| l.trim()).unwrap_or("");
    if first_line.starts_with('[')
        && first_line.ends_with(']')
        && !first_line.contains(',')
        && !first_line.contains(':')
    {
        return Format::Toml;
    }

    // JSON starts with { or [
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Format::Json;
    }

    // Check for TOML patterns
    let has_toml_sections = lines.iter().any(|line| {
        let l = line.trim();
        l.starts_with('[')
            && l.ends_with(']')
            && !l.starts_with("[[")
            && !l.contains(',')
            && !l.contains(':')
    });

    let has_toml_array_tables = lines
        .iter()
        .any(|line| line.trim().starts_with("[[") && line.trim().ends_with("]]"));

    let has_toml_assignments = lines.iter().any(|line| {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with('[') {
            return false;
        }
        if let Some(eq_pos) = l.find(" = ") {
            let key = &l[..eq_pos];
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '"')
        } else {
            false
        }
    });

    if has_toml_sections || has_toml_array_tables || has_toml_assignments {
        return Format::Toml;
    }

    // YAML detection
    if trimmed.starts_with("---") {
        return Format::Yaml;
    }

    let has_yaml_list = lines.iter().any(|line| {
        let l = line.trim();
        l.starts_with("- ") || l == "-"
    });

    let has_yaml_multiline = lines.iter().any(|line| {
        let l = line.trim();
        l.ends_with(": |") || l.ends_with(": >") || l.ends_with(": |+") || l.ends_with(": |-")
    });

    let has_yaml_nesting = lines
        .iter()
        .any(|line| line.starts_with("  ") && line.trim().contains(": "));

    if has_yaml_list || has_yaml_multiline || has_yaml_nesting {
        return Format::Yaml;
    }

    // Check for JSON literals
    if trimmed == "true" || trimmed == "false" || trimmed == "null" {
        return Format::Json;
    }

    if trimmed.parse::<f64>().is_ok() {
        return Format::Json;
    }

    // TOON: simple key: value without YAML complexity
    let has_colon_space = lines.iter().any(|line| {
        let l = line.trim();
        !l.is_empty() && !l.starts_with('#') && l.contains(": ")
    });

    if has_colon_space {
        return Format::Toon;
    }

    // Default to JSON
    Format::Json
}

/// Parse input in the detected format
pub fn parse_input(input: &str, format: Format) -> Result<Value, OqError> {
    match format {
        Format::Json => serde_json::from_str(input).map_err(OqError::JsonParse),
        Format::Yaml => serde_yaml::from_str(input).map_err(OqError::YamlParse),
        Format::Toml => {
            let toml_value: toml::Value =
                toml::from_str(input).map_err(|e| OqError::TomlParse(e.to_string()))?;
            toml_to_json(toml_value)
        }
        Format::Toon => {
            toon_format::decode_default(input).map_err(|e| OqError::ToonParse(e.to_string()))
        }
    }
}

/// Parse input with auto-detection
pub fn parse_auto(input: &str) -> Result<Value, OqError> {
    let format = detect_format(input);
    parse_input(input, format)
}

/// Convert TOML value to JSON value
fn toml_to_json(value: toml::Value) -> Result<Value, OqError> {
    match value {
        toml::Value::String(s) => Ok(Value::String(s)),
        toml::Value::Integer(i) => Ok(Value::Number(i.into())),
        toml::Value::Float(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .ok_or_else(|| OqError::TomlParse("Invalid float value".to_string())),
        toml::Value::Boolean(b) => Ok(Value::Bool(b)),
        toml::Value::Datetime(dt) => Ok(Value::String(dt.to_string())),
        toml::Value::Array(arr) => {
            let json_arr: Result<Vec<_>, _> = arr.into_iter().map(toml_to_json).collect();
            Ok(Value::Array(json_arr?))
        }
        toml::Value::Table(table) => {
            let mut map = serde_json::Map::new();
            for (k, v) in table {
                map.insert(k, toml_to_json(v)?);
            }
            Ok(Value::Object(map))
        }
    }
}

/// Convert JSON value to TOML value
fn json_to_toml(value: &Value) -> Result<toml::Value, OqError> {
    match value {
        Value::Null => Ok(toml::Value::String("null".to_string())),
        Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(toml::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(toml::Value::Float(f))
            } else {
                Err(OqError::TomlParse("Invalid number".to_string()))
            }
        }
        Value::String(s) => Ok(toml::Value::String(s.clone())),
        Value::Array(arr) => {
            let toml_arr: Result<Vec<_>, _> = arr.iter().map(json_to_toml).collect();
            Ok(toml::Value::Array(toml_arr?))
        }
        Value::Object(obj) => {
            let mut table = toml::map::Map::new();
            for (k, v) in obj {
                table.insert(k.clone(), json_to_toml(v)?);
            }
            Ok(toml::Value::Table(table))
        }
    }
}

/// Convert a JSON value to the specified format
pub fn encode_to_format(value: &Value, format: Format) -> Result<String, OqError> {
    match format {
        Format::Json => serde_json::to_string_pretty(value).map_err(OqError::JsonParse),
        Format::Yaml => serde_yaml::to_string(value).map_err(OqError::YamlParse),
        Format::Toml => {
            let toml_value = json_to_toml(value)?;
            toml::to_string_pretty(&toml_value).map_err(|e| OqError::TomlParse(e.to_string()))
        }
        Format::Toon => to_toon(value),
    }
}

/// Convert a JSON value to TOON string
pub fn to_toon(value: &Value) -> Result<String, OqError> {
    toon_format::encode_default(value).map_err(|e| OqError::ToonParse(e.to_string()))
}

/// Convert a JSON value to JSON string
pub fn to_json(value: &Value, pretty: bool) -> Result<String, OqError> {
    if pretty {
        serde_json::to_string_pretty(value).map_err(OqError::JsonParse)
    } else {
        serde_json::to_string(value).map_err(OqError::JsonParse)
    }
}

/// Convert a JSON value to YAML string
pub fn to_yaml(value: &Value) -> Result<String, OqError> {
    serde_yaml::to_string(value).map_err(OqError::YamlParse)
}

/// Convert a JSON value to TOML string
pub fn to_toml(value: &Value) -> Result<String, OqError> {
    let toml_value = json_to_toml(value)?;
    toml::to_string_pretty(&toml_value).map_err(|e| OqError::TomlParse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_json_object() {
        assert_eq!(detect_format(r#"{"name": "Ada"}"#), Format::Json);
    }

    #[test]
    fn test_detect_json_array() {
        assert_eq!(detect_format(r#"[1, 2, 3]"#), Format::Json);
    }

    #[test]
    fn test_detect_toon_object() {
        assert_eq!(detect_format("name: Ada\nage: 30"), Format::Toon);
    }

    #[test]
    fn test_detect_json_literals() {
        assert_eq!(detect_format("true"), Format::Json);
        assert_eq!(detect_format("false"), Format::Json);
        assert_eq!(detect_format("null"), Format::Json);
        assert_eq!(detect_format("42"), Format::Json);
        assert_eq!(detect_format("3.14"), Format::Json);
    }

    #[test]
    fn test_detect_yaml() {
        assert_eq!(detect_format("---\nname: Ada"), Format::Yaml);
        assert_eq!(detect_format("- item1\n- item2"), Format::Yaml);
        assert_eq!(detect_format("user:\n  name: Ada"), Format::Yaml);
    }

    #[test]
    fn test_detect_toml() {
        assert_eq!(detect_format("[package]\nname = \"test\""), Format::Toml);
        assert_eq!(detect_format("name = \"Alice\""), Format::Toml);
    }

    #[test]
    fn test_parse_json() {
        let input = r#"{"name": "Ada", "age": 30}"#;
        let result = parse_input(input, Format::Json).unwrap();
        assert_eq!(result["name"], "Ada");
        assert_eq!(result["age"], 30);
    }

    #[test]
    fn test_parse_yaml() {
        let input = "name: Ada\nage: 30";
        let result = parse_input(input, Format::Yaml).unwrap();
        assert_eq!(result["name"], "Ada");
        assert_eq!(result["age"], 30);
    }

    #[test]
    fn test_parse_toml() {
        let input = "name = \"Ada\"\nage = 30";
        let result = parse_input(input, Format::Toml).unwrap();
        assert_eq!(result["name"], "Ada");
        assert_eq!(result["age"], 30);
    }

    #[test]
    fn test_parse_toon() {
        let input = "name: Ada\nage: 30";
        let result = parse_input(input, Format::Toon).unwrap();
        assert_eq!(result["name"], "Ada");
        assert_eq!(result["age"], 30);
    }

    #[test]
    fn test_roundtrip_formats() {
        let json = serde_json::json!({"name": "Ada", "active": true});

        // JSON roundtrip
        let json_str = to_json(&json, false).unwrap();
        let back: Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json, back);

        // YAML roundtrip
        let yaml_str = to_yaml(&json).unwrap();
        let back: Value = serde_yaml::from_str(&yaml_str).unwrap();
        assert_eq!(json, back);

        // TOON roundtrip
        let toon_str = to_toon(&json).unwrap();
        let back = parse_input(&toon_str, Format::Toon).unwrap();
        assert_eq!(json, back);
    }
}
