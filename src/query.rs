//! jq-compatible query engine using jaq

use crate::OqError;
use serde_json::Value;

/// A compiled jq filter
pub struct CompiledFilter {
    filter: jaq_core::Filter<jaq_core::data::JustLut<jaq_json::Val>>,
}

/// Compile a jq filter expression
pub fn compile_filter(code: &str) -> Result<CompiledFilter, OqError> {
    use jaq_core::load::{Arena, File, Loader};

    let arena = Arena::default();
    let loader = Loader::new(jaq_std::defs().chain(jaq_json::defs()));

    let modules = loader
        .load(&arena, File { path: (), code })
        .map_err(|e| OqError::Compile(format!("{:?}", e)))?;

    let filter = jaq_core::Compiler::default()
        .with_funs(jaq_std::funs().chain(jaq_json::funs()))
        .compile(modules)
        .map_err(|e| OqError::Compile(format!("{:?}", e)))?;

    Ok(CompiledFilter { filter })
}

/// Run a compiled filter on a JSON value
pub fn run_filter(filter: &CompiledFilter, input: Value) -> Result<Vec<Value>, OqError> {
    use jaq_core::{unwrap_valr, Ctx, Vars};

    // Convert serde_json::Value to jaq_json::Val
    let input: jaq_json::Val = input.into();
    let mut results = Vec::new();

    // Create context for filter execution
    let ctx = Ctx::<jaq_core::data::JustLut<jaq_json::Val>>::new(&filter.filter.lut, Vars::new([]));

    // Run the filter
    for output in filter.filter.id.run((ctx.clone(), input)) {
        match unwrap_valr(output) {
            Ok(val) => {
                // Convert jaq_json::Val back to serde_json::Value
                let json_val: Value = (&val)
                    .try_into()
                    .map_err(|_| OqError::Filter("Failed to convert output to JSON".to_string()))?;
                results.push(json_val);
            }
            Err(e) => {
                return Err(OqError::Filter(format!("{}", e)));
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_identity_filter() {
        let filter = compile_filter(".").unwrap();
        let input = json!({"name": "Ada"});
        let results = run_filter(&filter, input.clone()).unwrap();
        assert_eq!(results, vec![input]);
    }

    #[test]
    fn test_field_access() {
        let filter = compile_filter(".name").unwrap();
        let input = json!({"name": "Ada", "age": 30});
        let results = run_filter(&filter, input).unwrap();
        assert_eq!(results, vec![json!("Ada")]);
    }

    #[test]
    fn test_array_index() {
        let filter = compile_filter(".[1]").unwrap();
        let input = json!([1, 2, 3]);
        let results = run_filter(&filter, input).unwrap();
        assert_eq!(results, vec![json!(2)]);
    }

    #[test]
    fn test_array_iterate() {
        let filter = compile_filter(".[]").unwrap();
        let input = json!([1, 2, 3]);
        let results = run_filter(&filter, input).unwrap();
        assert_eq!(results, vec![json!(1), json!(2), json!(3)]);
    }

    #[test]
    fn test_pipe() {
        let filter = compile_filter(".items | .[0]").unwrap();
        let input = json!({"items": ["a", "b", "c"]});
        let results = run_filter(&filter, input).unwrap();
        assert_eq!(results, vec![json!("a")]);
    }

    #[test]
    fn test_map() {
        let filter = compile_filter(".[] | . + 1").unwrap();
        let input = json!([1, 2, 3]);
        let results = run_filter(&filter, input).unwrap();
        assert_eq!(results, vec![json!(2), json!(3), json!(4)]);
    }

    #[test]
    fn test_select() {
        let filter = compile_filter(".[] | select(. > 2)").unwrap();
        let input = json!([1, 2, 3, 4]);
        let results = run_filter(&filter, input).unwrap();
        assert_eq!(results, vec![json!(3), json!(4)]);
    }

    #[test]
    fn test_keys() {
        let filter = compile_filter("keys").unwrap();
        let input = json!({"b": 1, "a": 2});
        let results = run_filter(&filter, input).unwrap();
        // keys returns sorted keys
        assert_eq!(results, vec![json!(["a", "b"])]);
    }

    #[test]
    fn test_length() {
        let filter = compile_filter("length").unwrap();
        let input = json!([1, 2, 3]);
        let results = run_filter(&filter, input).unwrap();
        assert_eq!(results, vec![json!(3)]);
    }

    #[test]
    fn test_nested_access() {
        let filter = compile_filter(".user.profile.name").unwrap();
        let input = json!({
            "user": {
                "profile": {
                    "name": "Ada"
                }
            }
        });
        let results = run_filter(&filter, input).unwrap();
        assert_eq!(results, vec![json!("Ada")]);
    }
}
