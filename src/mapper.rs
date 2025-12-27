//! Pluggable mappers for transforming JSON tool responses to TOON
//!
//! Mappers pre-process JSON before TOON encoding to:
//! - Select relevant fields
//! - Restructure data for better TOON representation
//! - Apply jq filters for custom transformations
//!
//! # Example
//!
//! ```rust,ignore
//! use oq::mapper::{MapperRegistry, JqMapper};
//!
//! let mut registry = MapperRegistry::new();
//!
//! // Register a jq-based mapper for search results
//! registry.register("search_*", JqMapper::new(".results | map({title, score})"));
//!
//! // Transform tool response
//! let toon = registry.transform("search_notes", json_response)?;
//! ```

use crate::query::{compile_filter, run_filter, CompiledFilter};
use crate::OqError;
use serde_json::Value;
use std::collections::HashMap;

/// Trait for transforming JSON before TOON encoding
pub trait Mapper: Send + Sync {
    /// Transform the JSON value
    fn transform(&self, value: Value) -> Result<Value, OqError>;

    /// Optional description for debugging
    fn description(&self) -> &str {
        "custom mapper"
    }
}

/// A mapper that uses a jq filter expression
pub struct JqMapper {
    filter: CompiledFilter,
    expression: String,
}

impl JqMapper {
    /// Create a new jq-based mapper
    pub fn new(expression: &str) -> Result<Self, OqError> {
        let filter = compile_filter(expression)?;
        Ok(Self {
            filter,
            expression: expression.to_string(),
        })
    }
}

impl Mapper for JqMapper {
    fn transform(&self, value: Value) -> Result<Value, OqError> {
        let results = run_filter(&self.filter, value)?;

        // If single result, return it directly; otherwise wrap in array
        match results.len() {
            0 => Ok(Value::Null),
            1 => Ok(results.into_iter().next().unwrap()),
            _ => Ok(Value::Array(results)),
        }
    }

    fn description(&self) -> &str {
        &self.expression
    }
}

/// Identity mapper - passes through unchanged
pub struct IdentityMapper;

impl Mapper for IdentityMapper {
    fn transform(&self, value: Value) -> Result<Value, OqError> {
        Ok(value)
    }

    fn description(&self) -> &str {
        "identity"
    }
}

/// Mapper that selects specific fields from objects
pub struct FieldSelectMapper {
    fields: Vec<String>,
}

impl FieldSelectMapper {
    pub fn new(fields: Vec<String>) -> Self {
        Self { fields }
    }
}

impl Mapper for FieldSelectMapper {
    fn transform(&self, value: Value) -> Result<Value, OqError> {
        match value {
            Value::Object(map) => {
                let filtered: serde_json::Map<String, Value> = map
                    .into_iter()
                    .filter(|(k, _)| self.fields.contains(k))
                    .collect();
                Ok(Value::Object(filtered))
            }
            Value::Array(arr) => {
                let filtered: Vec<Value> = arr
                    .into_iter()
                    .map(|v| self.transform(v))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Array(filtered))
            }
            other => Ok(other),
        }
    }

    fn description(&self) -> &str {
        "field select"
    }
}

/// Mapper that truncates long strings
pub struct TruncateMapper {
    max_length: usize,
    suffix: String,
}

impl TruncateMapper {
    pub fn new(max_length: usize) -> Self {
        Self {
            max_length,
            suffix: "...".to_string(),
        }
    }

    pub fn with_suffix(mut self, suffix: &str) -> Self {
        self.suffix = suffix.to_string();
        self
    }
}

impl Mapper for TruncateMapper {
    fn transform(&self, value: Value) -> Result<Value, OqError> {
        Ok(truncate_strings(value, self.max_length, &self.suffix))
    }

    fn description(&self) -> &str {
        "truncate"
    }
}

fn truncate_strings(value: Value, max_length: usize, suffix: &str) -> Value {
    match value {
        Value::String(s) if s.len() > max_length => {
            let truncated = s
                .chars()
                .take(max_length - suffix.len())
                .collect::<String>();
            Value::String(format!("{}{}", truncated, suffix))
        }
        Value::Array(arr) => Value::Array(
            arr.into_iter()
                .map(|v| truncate_strings(v, max_length, suffix))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, truncate_strings(v, max_length, suffix)))
                .collect(),
        ),
        other => other,
    }
}

/// Mapper that limits array lengths
pub struct LimitMapper {
    max_items: usize,
}

impl LimitMapper {
    pub fn new(max_items: usize) -> Self {
        Self { max_items }
    }
}

impl Mapper for LimitMapper {
    fn transform(&self, value: Value) -> Result<Value, OqError> {
        Ok(limit_arrays(value, self.max_items))
    }

    fn description(&self) -> &str {
        "limit"
    }
}

fn limit_arrays(value: Value, max_items: usize) -> Value {
    match value {
        Value::Array(arr) => {
            let limited: Vec<Value> = arr
                .into_iter()
                .take(max_items)
                .map(|v| limit_arrays(v, max_items))
                .collect();
            Value::Array(limited)
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, limit_arrays(v, max_items)))
                .collect(),
        ),
        other => other,
    }
}

/// Compose multiple mappers in sequence
pub struct ChainMapper {
    mappers: Vec<Box<dyn Mapper>>,
}

impl ChainMapper {
    pub fn new() -> Self {
        Self {
            mappers: Vec::new(),
        }
    }

    pub fn then<M: Mapper + 'static>(mut self, mapper: M) -> Self {
        self.mappers.push(Box::new(mapper));
        self
    }
}

impl Default for ChainMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl Mapper for ChainMapper {
    fn transform(&self, mut value: Value) -> Result<Value, OqError> {
        for mapper in &self.mappers {
            value = mapper.transform(value)?;
        }
        Ok(value)
    }

    fn description(&self) -> &str {
        "chain"
    }
}

/// Registry of mappers by tool name pattern
pub struct MapperRegistry {
    /// Exact match mappers
    exact: HashMap<String, Box<dyn Mapper>>,
    /// Prefix match mappers (pattern ends with *)
    prefix: Vec<(String, Box<dyn Mapper>)>,
    /// Default mapper when no match
    default: Box<dyn Mapper>,
}

impl MapperRegistry {
    /// Create a new registry with identity as default
    pub fn new() -> Self {
        Self {
            exact: HashMap::new(),
            prefix: Vec::new(),
            default: Box::new(IdentityMapper),
        }
    }

    /// Set the default mapper
    pub fn set_default<M: Mapper + 'static>(&mut self, mapper: M) {
        self.default = Box::new(mapper);
    }

    /// Register a mapper for a tool name or pattern
    ///
    /// Patterns ending with `*` match any tool name with that prefix.
    pub fn register<M: Mapper + 'static>(&mut self, pattern: &str, mapper: M) {
        if pattern.ends_with('*') {
            let prefix = pattern.trim_end_matches('*').to_string();
            self.prefix.push((prefix, Box::new(mapper)));
        } else {
            self.exact.insert(pattern.to_string(), Box::new(mapper));
        }
    }

    /// Get the mapper for a tool name
    pub fn get(&self, tool_name: &str) -> &dyn Mapper {
        // Try exact match first
        if let Some(mapper) = self.exact.get(tool_name) {
            return mapper.as_ref();
        }

        // Try prefix match
        for (prefix, mapper) in &self.prefix {
            if tool_name.starts_with(prefix) {
                return mapper.as_ref();
            }
        }

        // Fall back to default
        self.default.as_ref()
    }

    /// Transform JSON for a specific tool
    pub fn transform(&self, tool_name: &str, value: Value) -> Result<Value, OqError> {
        self.get(tool_name).transform(value)
    }
}

impl Default for MapperRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a registry with common mappers for typical tool responses
pub fn default_registry() -> Result<MapperRegistry, OqError> {
    let mut registry = MapperRegistry::new();

    // Search results - extract just title, score, and snippet
    registry.register(
        "search_*",
        JqMapper::new(
            r#"if .results then {results: [.results[] | {title, score, snippet}], total: .total} else . end"#,
        )?,
    );

    // File listings - simplify to name and size
    registry.register(
        "list_*",
        JqMapper::new(r#"if type == "array" then [.[] | {name, size, type}] else . end"#)?,
    );

    // Read file - truncate content
    registry.register("read_*", ChainMapper::new().then(TruncateMapper::new(2000)));

    // API responses - limit arrays
    registry.register("api_*", ChainMapper::new().then(LimitMapper::new(10)));

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_jq_mapper() {
        let mapper = JqMapper::new(".name").unwrap();
        let input = json!({"name": "Ada", "age": 30});
        let result = mapper.transform(input).unwrap();
        assert_eq!(result, json!("Ada"));
    }

    #[test]
    fn test_field_select_mapper() {
        let mapper = FieldSelectMapper::new(vec!["name".to_string(), "age".to_string()]);
        let input = json!({"name": "Ada", "age": 30, "city": "London"});
        let result = mapper.transform(input).unwrap();
        assert_eq!(result, json!({"name": "Ada", "age": 30}));
    }

    #[test]
    fn test_truncate_mapper() {
        let mapper = TruncateMapper::new(10);
        let input = json!({"content": "This is a very long string that should be truncated"});
        let result = mapper.transform(input).unwrap();
        assert_eq!(result["content"], "This is...");
    }

    #[test]
    fn test_limit_mapper() {
        let mapper = LimitMapper::new(3);
        let input = json!([1, 2, 3, 4, 5]);
        let result = mapper.transform(input).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn test_chain_mapper() {
        let mapper = ChainMapper::new()
            .then(JqMapper::new(".items").unwrap())
            .then(LimitMapper::new(2));
        let input = json!({"items": [1, 2, 3, 4, 5]});
        let result = mapper.transform(input).unwrap();
        assert_eq!(result, json!([1, 2]));
    }

    #[test]
    fn test_registry_exact_match() {
        let mut registry = MapperRegistry::new();
        registry.register("my_tool", JqMapper::new(".data").unwrap());

        let input = json!({"data": 42, "meta": "ignored"});
        let result = registry.transform("my_tool", input).unwrap();
        assert_eq!(result, json!(42));
    }

    #[test]
    fn test_registry_prefix_match() {
        let mut registry = MapperRegistry::new();
        registry.register("search_*", JqMapper::new(".results").unwrap());

        let input = json!({"results": [1, 2, 3], "meta": "ignored"});
        let result = registry.transform("search_notes", input).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn test_registry_default() {
        let registry = MapperRegistry::new();

        let input = json!({"data": 42});
        let result = registry.transform("unknown_tool", input.clone()).unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn test_default_registry() {
        let registry = default_registry().unwrap();

        // Test search mapper
        let search_input = json!({
            "results": [
                {"title": "Note 1", "score": 0.9, "snippet": "content", "id": "123"},
                {"title": "Note 2", "score": 0.8, "snippet": "more", "id": "456"}
            ],
            "total": 2,
            "query_time_ms": 50
        });
        let search_result = registry.transform("search_notes", search_input).unwrap();
        assert!(search_result["results"][0].get("id").is_none());
        assert_eq!(search_result["results"][0]["title"], "Note 1");
    }
}
