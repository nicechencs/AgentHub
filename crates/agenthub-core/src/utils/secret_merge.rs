//! Keep stored secrets when a read-edit-save cycle sends `***` / empty / omitted
//! secret fields. Also restores `api_key = "***"` inside TOML `content`.

use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item};

use super::redact::{is_secret_key, is_unusable_secret};

/// When `new` carries a redaction marker, empty secret, or omits a secret key,
/// keep the corresponding value from `old`. Non-secret fields in `new` win.
pub fn merge_preserving_secrets(old: &Value, new: &Value) -> Value {
    match (old, new) {
        (Value::Object(old_map), Value::Object(new_map)) => {
            let opaque_toml = new_map.get("format").and_then(Value::as_str) == Some("toml")
                && new_map.get("content").is_some_and(Value::is_string);
            let mut out = Map::new();
            for (key, new_v) in new_map {
                if opaque_toml && key == "content" {
                    out.insert(
                        key.clone(),
                        merge_toml_content_values(old_map.get(key), new_v),
                    );
                    continue;
                }
                if should_keep_old_secret(key, new_v) {
                    if let Some(old_v) = old_map.get(key) {
                        out.insert(key.clone(), old_v.clone());
                    }
                    continue;
                }
                if let Some(old_v) = old_map.get(key) {
                    if new_v.is_object() && old_v.is_object() {
                        out.insert(key.clone(), merge_preserving_secrets(old_v, new_v));
                        continue;
                    }
                }
                out.insert(key.clone(), new_v.clone());
            }
            for (key, old_v) in old_map {
                if out.contains_key(key) {
                    continue;
                }
                if is_secret_key(key) {
                    out.insert(key.clone(), old_v.clone());
                    continue;
                }
                if old_v.is_object() {
                    let nested = merge_preserving_secrets(old_v, &Value::Object(Map::new()));
                    if nested.as_object().is_some_and(|map| !map.is_empty()) {
                        out.insert(key.clone(), nested);
                    }
                }
            }
            Value::Object(out)
        }
        (_, new_v) => new_v.clone(),
    }
}

fn should_keep_old_secret(key: &str, new_v: &Value) -> bool {
    is_secret_key(key) && value_is_unusable_secret(new_v)
}

fn value_is_unusable_secret(value: &Value) -> bool {
    matches!(value, Value::String(s) if is_unusable_secret(s))
}

fn merge_toml_content_values(old: Option<&Value>, new: &Value) -> Value {
    let Some(new_s) = new.as_str() else {
        return new.clone();
    };
    let Some(old_s) = old.and_then(Value::as_str) else {
        return new.clone();
    };
    Value::String(merge_toml_preserving_secrets(old_s, new_s))
}

fn merge_toml_preserving_secrets(old: &str, new: &str) -> String {
    if is_unusable_secret(new) {
        return old.to_owned();
    }
    let Ok(mut new_doc) = new.parse::<DocumentMut>() else {
        return if toml_has_redacted_secret_assignment(new) {
            old.to_owned()
        } else {
            new.to_owned()
        };
    };
    let Ok(old_doc) = old.parse::<DocumentMut>() else {
        return new.to_owned();
    };
    restore_toml_table_secrets(new_doc.as_table_mut(), old_doc.as_table());
    new_doc.to_string()
}

fn toml_has_redacted_secret_assignment(content: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        let Some((key, value)) = trimmed.split_once('=') else {
            return false;
        };
        is_secret_key(key.trim()) && is_unusable_secret(value.trim().trim_matches(['"', '\'']))
    })
}

fn restore_toml_table_secrets(new: &mut toml_edit::Table, old: &toml_edit::Table) {
    let old_keys: Vec<String> = old.iter().map(|(key, _)| key.to_string()).collect();
    for key in old_keys {
        let Some(old_item) = old.get(&key) else {
            continue;
        };
        if is_secret_key(&key) {
            let replace = new.get(&key).is_none_or(item_is_unusable_secret);
            if replace {
                new[&key] = old_item.clone();
            }
            continue;
        }
        match (new.get_mut(&key), old_item) {
            (Some(new_item), old_item) => restore_toml_item_secrets(new_item, old_item),
            _ => {}
        }
    }
}

fn restore_toml_item_secrets(new: &mut Item, old: &Item) {
    match (new, old) {
        (Item::Table(new_table), Item::Table(old_table)) => {
            restore_toml_table_secrets(new_table, old_table);
        }
        (Item::Value(new_value), Item::Value(old_value)) => {
            restore_toml_value_secrets(new_value, old_value);
        }
        (Item::ArrayOfTables(new_arr), Item::ArrayOfTables(old_arr)) => {
            for (new_table, old_table) in new_arr.iter_mut().zip(old_arr.iter()) {
                restore_toml_table_secrets(new_table, old_table);
            }
        }
        _ => {}
    }
}

fn restore_toml_value_secrets(new: &mut toml_edit::Value, old: &toml_edit::Value) {
    match (new, old) {
        (toml_edit::Value::InlineTable(new_table), toml_edit::Value::InlineTable(old_table)) => {
            let old_keys: Vec<String> = old_table.iter().map(|(key, _)| key.to_string()).collect();
            for key in old_keys {
                let Some(old_value) = old_table.get(&key) else {
                    continue;
                };
                if is_secret_key(&key) {
                    let replace = new_table
                        .get(&key)
                        .is_none_or(|value| value.as_str().is_some_and(is_unusable_secret));
                    if replace {
                        new_table.insert(&key, old_value.clone());
                    }
                    continue;
                }
                if let (Some(new_value), old_value) = (new_table.get_mut(&key), old_value) {
                    restore_toml_value_secrets(new_value, old_value);
                }
            }
        }
        (toml_edit::Value::Array(new_arr), toml_edit::Value::Array(old_arr)) => {
            for (new_value, old_value) in new_arr.iter_mut().zip(old_arr.iter()) {
                restore_toml_value_secrets(new_value, old_value);
            }
        }
        _ => {}
    }
}

fn item_is_unusable_secret(item: &Item) -> bool {
    item.as_value()
        .and_then(toml_edit::Value::as_str)
        .is_some_and(is_unusable_secret)
}

#[cfg(test)]
mod tests;
