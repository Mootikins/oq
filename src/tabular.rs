//! Tabular TOON encoding for compact array-of-objects representation
//!
//! Encodes arrays of objects as compact TOON tables:
//!
//! ```text
//! notes[3]{path,line,similarity}:
//!   Projects/MyProject.md,42,0.95
//!   Ideas/Feature.md,18,0.87
//!   Research/Topic.md,7,0.82
//! ```
//!
//! This format is more compact than full TOON object encoding and easier
//! to read for tabular data like search results.

use serde_json::Value;

/// Encode an array of JSON objects as a compact TOON table
///
/// # Arguments
///
/// * `name` - The name of the collection (e.g., "notes", "results")
/// * `items` - Array of JSON objects to encode
/// * `columns` - Column names to extract from each object, in order
///
/// # Returns
///
/// A TOON-formatted table string with header and rows, or empty string if items is empty.
///
/// # Format
///
/// ```text
/// {name}[{count}]{{columns}}:
///   value1,value2,value3
///   value1,value2,value3
/// ```
///
/// # Examples
///
/// ```rust
/// use oq::encode_table;
/// use serde_json::json;
///
/// let items = vec![
///     json!({"path": "a.md", "line": 42, "sim": 0.95}),
///     json!({"path": "b.md", "line": 18, "sim": 0.87}),
/// ];
/// let result = encode_table("notes", &items, &["path", "line", "sim"]);
/// assert_eq!(result, "notes[2]{path,line,sim}:\n  a.md,42,0.95\n  b.md,18,0.87");
/// ```
pub fn encode_table(name: &str, items: &[Value], columns: &[&str]) -> String {
    if items.is_empty() {
        return String::new();
    }

    let mut result = String::new();

    // Header: name[count]{col1,col2,...}:
    result.push_str(&format!(
        "{}[{}]{{{}}}:\n",
        name,
        items.len(),
        columns.join(",")
    ));

    // Rows
    for item in items {
        result.push_str("  ");
        let values: Vec<String> = columns
            .iter()
            .map(|col| format_value(item.get(*col)))
            .collect();
        result.push_str(&values.join(","));
        result.push('\n');
    }

    // Remove trailing newline
    result.pop();

    result
}

/// Format a JSON value for TOON table cell
fn format_value(value: Option<&Value>) -> String {
    match value {
        None => String::new(),
        Some(v) => match v {
            Value::String(s) => {
                // Quote if contains comma
                if s.contains(',') {
                    format!("\"{}\"", s)
                } else {
                    s.clone()
                }
            }
            Value::Number(n) => {
                // Format floats nicely (avoid excessive decimals)
                if let Some(f) = n.as_f64() {
                    if f.fract() == 0.0 {
                        format!("{}", f as i64)
                    } else {
                        // Trim trailing zeros
                        let formatted = format!("{:.10}", f);
                        formatted
                            .trim_end_matches('0')
                            .trim_end_matches('.')
                            .to_string()
                    }
                } else {
                    n.to_string()
                }
            }
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            _ => v.to_string(),
        },
    }
}

#[cfg(test)]
mod tabular_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_encode_table_single_row() {
        let items = vec![json!({"path": "a.md", "line": 42, "sim": 0.95})];
        let result = encode_table("notes", &items, &["path", "line", "sim"]);
        assert_eq!(result, "notes[1]{path,line,sim}:\n  a.md,42,0.95");
    }

    #[test]
    fn test_encode_table_multiple_rows() {
        let items = vec![
            json!({"path": "a.md", "line": 10, "sim": 0.95}),
            json!({"path": "b.md", "line": 20, "sim": 0.87}),
            json!({"path": "c.md", "line": 30, "sim": 0.72}),
        ];
        let result = encode_table("notes", &items, &["path", "line", "sim"]);
        let expected = "notes[3]{path,line,sim}:\n  a.md,10,0.95\n  b.md,20,0.87\n  c.md,30,0.72";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_encode_table_empty() {
        let items: Vec<Value> = vec![];
        let result = encode_table("notes", &items, &["path", "line", "sim"]);
        assert_eq!(result, "");
    }

    #[test]
    fn test_encode_table_escapes_commas() {
        // Value containing comma should be quoted
        let items = vec![json!({"name": "Hello, World", "id": 1})];
        let result = encode_table("items", &items, &["name", "id"]);
        assert!(result.contains("\"Hello, World\"") || result.contains("'Hello, World'"));
    }

    #[test]
    fn test_encode_table_handles_floats() {
        // Float precision should be reasonable (not 0.9500000000001)
        let items = vec![json!({"score": 0.95})];
        let result = encode_table("scores", &items, &["score"]);
        assert!(result.contains("0.95"));
        assert!(!result.contains("0.950000"));
    }
}
