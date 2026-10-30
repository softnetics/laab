//! "รายงานปัญหา": a report the user writes, reads in full, and sends.
//!
//! [`preview`] builds the text; the dialog shows it in an editable box and
//! sends exactly what is in the box. A message is attached as its shape (keys
//! and value types) unless the user also ticks its content.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub message_id: Option<String>,
    /// The message as LINE sent it, text and all.
    pub content: bool,
    pub log: bool,
}

/// Keys and types only: `{"text": "string", "contentMetadata": {"STKID": "string"}}`.
pub fn shape(v: &Value) -> Value {
    match v {
        Value::Null => json!("null"),
        Value::Bool(_) => json!("bool"),
        Value::Number(_) => json!("number"),
        Value::String(_) => json!("string"),
        Value::Array(items) => {
            let mut out: Vec<Value> = items.iter().take(3).map(shape).collect();
            if items.len() > 3 {
                out.push(json!(format!("… {} more", items.len() - 3)));
            }
            Value::Array(out)
        }
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), shape(v))).collect()),
    }
}

/// The message's raw JSON from LINE, or LAAB's own copy in demo mode.
fn message_json(state: &AppState, id: &str) -> Option<Value> {
    let store = state.store();
    if let Some(raw) = store.raw_message(id).ok().flatten()
        && let Ok(v) = serde_json::from_str(&raw)
    {
        return Some(v);
    }
    store.message(id).ok().flatten().and_then(|m| serde_json::to_value(m).ok())
}

pub fn preview(state: &AppState, req: &Request) -> String {
    let mut report = json!({
        "app_version": state.app.package_info().version.to_string(),
        "breadcrumbs": crate::usage::crumbs(),
    });
    if let Some(m) = req.message_id.as_deref().and_then(|id| message_json(state, id)) {
        report["message_shape"] = shape(&m);
        if req.content {
            report["message"] = m;
        }
    }
    if req.log {
        report["log"] = json!(crate::logfile::tail(200));
    }
    serde_json::to_string_pretty(&report).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_has_no_values() {
        let m = json!({"text": "ความลับ", "contentMetadata": {"STKID": "52002734"}, "to": "u123",
                       "list": [1, 2, 3, 4, 5], "seen": true});
        let s = shape(&m).to_string();
        for secret in ["ความลับ", "52002734", "u123"] {
            assert!(!s.contains(secret));
        }
        assert_eq!(shape(&m)["contentMetadata"]["STKID"], "string");
        assert_eq!(shape(&m)["list"].as_array().unwrap().len(), 4);
    }
}
