//! Lecture de chemins dans une réponse JSON/XML et évaluation des checks.

use crate::models::{Check, CheckOp};
use serde_json::Value;

/// `data.items[0].id`, `data.items.0.id`, `element.@attribut`, `element.#text`.
pub fn get_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.trim().is_empty() {
        return Some(value);
    }
    let mut current = value;
    let normalized = path.replace('[', ".").replace(']', "");
    for key in normalized.split('.').filter(|key| !key.is_empty()) {
        current = match current {
            Value::Object(map) => map.get(key)?,
            Value::Array(items) => {
                let index = key.parse::<usize>().ok()?;
                items.get(index)?
            }
            _ => return None,
        };
    }
    Some(current)
}

/// Reproduit `String(valeur)` de JavaScript (utilisé par `equals`/`notEquals`).
pub fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::Bool(bool)) => bool.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".to_string(),
    }
}

fn js_number(value: Option<&Value>) -> f64 {
    match value {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(true)) => 1.0,
        Some(Value::Bool(false)) => 0.0,
        Some(Value::Number(number)) => number.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                0.0
            } else {
                trimmed.parse::<f64>().unwrap_or(f64::NAN)
            }
        }
        Some(Value::Array(_)) | Some(Value::Object(_)) => f64::NAN,
    }
}

pub fn eval_check(check: &Check, body: &Value) -> bool {
    let actual = get_path(body, &check.path);
    let expected = check.value.as_str();
    match check.op {
        CheckOp::Exists => matches!(actual, Some(value) if !value.is_null()),
        CheckOp::NotEmpty => match actual {
            None | Some(Value::Null) => false,
            Some(Value::String(text)) => !text.is_empty(),
            Some(Value::Array(items)) => !items.is_empty(),
            Some(Value::Object(map)) => !map.is_empty(),
            _ => true,
        },
        CheckOp::IsArray => matches!(actual, Some(Value::Array(_))),
        CheckOp::IsNumber => matches!(actual, Some(Value::Number(_))),
        CheckOp::Contains => match actual {
            None | Some(Value::Null) => false,
            Some(value) => serde_json::to_string(value)
                .map(|json| json.contains(expected))
                .unwrap_or(false),
        },
        CheckOp::NotEquals => js_string(actual) != expected,
        CheckOp::LessThan => js_number(actual) < js_number(Some(&Value::String(expected.into()))),
        CheckOp::GreaterThan => {
            js_number(actual) > js_number(Some(&Value::String(expected.into())))
        }
        CheckOp::Equals => js_string(actual) == expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check(path: &str, op: CheckOp, value: &str) -> Check {
        Check {
            path: path.into(),
            op,
            value: value.into(),
            ..Check::default()
        }
    }

    #[test]
    fn paths() {
        let body = json!({"data": {"items": [{"id": 7}], "flag": false}});
        assert_eq!(get_path(&body, "data.items[0].id"), Some(&json!(7)));
        assert_eq!(get_path(&body, "data.items.0.id"), Some(&json!(7)));
        assert_eq!(get_path(&body, "data.flag"), Some(&json!(false)));
        assert_eq!(get_path(&body, "data.missing"), None);
    }

    #[test]
    fn checks_match_js_semantics() {
        let body = json!({
            "status": {"code": 200, "message": "SUCCESS"},
            "data": {"items": [1, 2], "balance": 12.5, "empty": []},
            "nil": null
        });
        assert!(eval_check(&check("status.code", CheckOp::Equals, "200"), &body));
        assert!(eval_check(&check("status.message", CheckOp::Equals, "SUCCESS"), &body));
        assert!(eval_check(&check("data.items", CheckOp::IsArray, ""), &body));
        assert!(eval_check(&check("data.balance", CheckOp::IsNumber, ""), &body));
        assert!(!eval_check(&check("nil", CheckOp::Exists, ""), &body));
        assert!(eval_check(&check("nil", CheckOp::Equals, "null"), &body));
        assert!(!eval_check(&check("data.empty", CheckOp::NotEmpty, ""), &body));
        assert!(eval_check(&check("data.items", CheckOp::Contains, "2"), &body));
        assert!(eval_check(&check("data.balance", CheckOp::GreaterThan, "10"), &body));
        assert!(eval_check(&check("status.code", CheckOp::NotEquals, "500"), &body));
    }
}
