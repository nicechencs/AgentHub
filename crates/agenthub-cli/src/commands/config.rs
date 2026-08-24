use agenthub_core::error::Result;
use agenthub_core::services::settings_service::{SETTINGS_READONLY, SETTINGS_WHITELIST};
use agenthub_core::AgentHub;

use crate::output::{print_json, OutputFormat};

pub fn path(hub: &AgentHub, format: OutputFormat) -> Result<()> {
    let info = hub.settings.path_info();
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => print_json(&info),
        OutputFormat::Table => {
            println!("data_dir   : {}", info.data_dir);
            println!("db_path    : {}", info.db_path);
            println!("backups_dir: {}", info.backups_dir);
            println!("logs_dir   : {}", info.logs_dir);
            Ok(())
        }
    }
}

pub fn get(hub: &AgentHub, key: Option<&str>, format: OutputFormat) -> Result<()> {
    match key {
        None => {
            let all = hub.settings.get_all()?;
            let app_version = hub.settings.get("app_version")?;
            match format {
                OutputFormat::Quiet => Ok(()),
                OutputFormat::Json => {
                    let mut value = serde_json::to_value(&all)?;
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert(
                            "appVersion".into(),
                            serde_json::json!(app_version.clone().unwrap_or_default()),
                        );
                    }
                    print_json(&value)
                }
                OutputFormat::Table => {
                    println!("theme                      : {}", all.theme);
                    println!("language                   : {}", all.language);
                    println!("log_level                  : {}", all.log_level);
                    println!("log_retention_days         : {}", all.log_retention_days);
                    println!("skill_market_source        : {}", all.skill_market_source);
                    println!("close_to_tray              : {}", all.close_to_tray);
                    println!(
                        "usage_collect_interval_min : {}",
                        all.usage_collect_interval_min
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "(unset)".into())
                    );
                    println!(
                        "app_version                : {} (read-only)",
                        app_version.unwrap_or_else(|| "-".into())
                    );
                    Ok(())
                }
            }
        }
        Some(k) => {
            let v = hub.settings.get(k)?;
            match format {
                OutputFormat::Quiet => Ok(()),
                OutputFormat::Json => print_json(&serde_json::json!({ "key": k, "value": v })),
                OutputFormat::Table => {
                    match v {
                        Some(val) => println!("{val}"),
                        None => println!("(unset)"),
                    }
                    Ok(())
                }
            }
        }
    }
}

/// Keys `config get` may return (whitelist + read-only).
// Referenced only from `tests.rs` in this crate; keep for test coverage.
#[allow(dead_code)]
pub fn visible_config_keys() -> Vec<&'static str> {
    let mut keys = SETTINGS_WHITELIST.to_vec();
    keys.extend_from_slice(SETTINGS_READONLY);
    keys
}

#[cfg(test)]
mod tests;

pub fn set(hub: &AgentHub, key: &str, value: &str, format: OutputFormat) -> Result<()> {
    hub.settings.set(key, value)?;
    match format {
        OutputFormat::Quiet => Ok(()),
        OutputFormat::Json => {
            print_json(&serde_json::json!({ "ok": true, "key": key, "value": value }))
        }
        OutputFormat::Table => {
            println!("set {key} = {value}");
            Ok(())
        }
    }
}
