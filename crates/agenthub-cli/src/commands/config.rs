use agenthub_core::error::Result;
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
            match format {
                OutputFormat::Quiet => Ok(()),
                OutputFormat::Json => print_json(&all),
                OutputFormat::Table => {
                    println!("theme              : {}", all.theme);
                    println!("language           : {}", all.language);
                    println!("log_level          : {}", all.log_level);
                    println!("log_retention_days : {}", all.log_retention_days);
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
