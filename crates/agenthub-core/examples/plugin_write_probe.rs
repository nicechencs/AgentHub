use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use agenthub_core::models::AgentId;
use agenthub_core::services::plugin_inventory::SystemPluginCliRunner;
use agenthub_core::services::{
    disable_plugin_with_v2, enable_plugin_with_v2, install_plugin_with_v2,
    list_available_plugins_with_v2, list_plugin_inventory_with_v2, preview_plugin_install_with_v2,
    uninstall_plugin_with_v2, PluginApplyContext, PluginApplyContextV2, PluginInstallOptions,
    PluginScanContext, PluginScanContextV2, PluginUninstallOptions,
};
use serde::Serialize;
use serde_json::Value as JsonValue;

const CODEX_NAME: &str = "agenthub-probe-plugin";
const CODEX_MARKET: &str = "agenthub-probe-market";
const CODEX_SOURCE: &str = "agenthub-probe-plugin@agenthub-probe-market";
const PI_NAME: &str = "agenthub-pi-probe-package";

#[derive(Serialize)]
struct ProbeEvidence {
    schema: &'static str,
    status: &'static str,
    codex: CodexEvidence,
    pi: PiEvidence,
}

#[derive(Serialize)]
struct CodexEvidence {
    available_listed: bool,
    preview_matches: bool,
    confirmation_guarded: bool,
    invalid_source_rejected: bool,
    installed_listed: bool,
    disabled_listed: bool,
    enabled_listed: bool,
    failed_install_rolled_back: bool,
    removed: bool,
}

#[derive(Serialize)]
struct PiEvidence {
    preview_matches: bool,
    relative_local_rejected: bool,
    confirmation_guarded: bool,
    invalid_source_rejected: bool,
    installed_listed: bool,
    inventory_source_exact: bool,
    toggle_rejected: bool,
    failed_install_rolled_back: bool,
    removed: bool,
}

fn required_path(name: &str) -> Result<PathBuf, String> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| format!("{name} must be an absolute path"))
}

fn snapshot(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if path.is_file() {
        fs::read(path)
            .map(Some)
            .map_err(|error| format!("snapshot failed: {error}"))
    } else {
        Ok(None)
    }
}

fn require(value: bool, message: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

fn configured_pi_source(settings_path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(settings_path)
        .map_err(|error| format!("read Pi settings failed: {error}"))?;
    let value: JsonValue = serde_json::from_str(&text)
        .map_err(|error| format!("parse Pi settings failed: {error}"))?;
    let packages = value
        .get("packages")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| "Pi settings packages are missing".to_string())?;
    require(packages.len() == 1, "Pi settings must contain one package")?;
    packages[0]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Pi package source is not a string".to_string())
}

fn main() -> Result<(), String> {
    let user_home = required_path("HOME")?;
    let codex_home = required_path("CODEX_HOME")?;
    let pi_config = required_path("PI_CODING_AGENT_DIR")?;
    let codex_bin = required_path("AGENTHUB_PLUGIN_PROBE_CODEX_BIN")?;
    let pi_bin = required_path("AGENTHUB_PLUGIN_PROBE_PI_BIN")?;
    let pi_package = required_path("AGENTHUB_PLUGIN_PROBE_PI_PACKAGE")?;
    require(
        pi_package.join("package.json").is_file(),
        "Pi fixture missing",
    )?;

    let scratch = user_home
        .parent()
        .ok_or_else(|| "isolated home has no parent".to_string())?;
    for path in [&codex_home, &pi_config] {
        require(
            path.starts_with(scratch),
            "plugin directory escaped isolated scratch",
        )?;
    }

    let runner = SystemPluginCliRunner;
    let apply = PluginApplyContextV2 {
        base: PluginApplyContext {
            user_home: user_home.clone(),
            claude_home: user_home.join(".claude"),
            grok_home: user_home.join(".grok"),
            claude_bin: None,
            grok_bin: None,
            runner: &runner,
        },
        codex_home: codex_home.clone(),
        pi_config: pi_config.clone(),
        codex_bin: Some(codex_bin.clone()),
        pi_bin: Some(pi_bin),
    };
    let scan = PluginScanContextV2 {
        base: PluginScanContext {
            user_home: user_home.clone(),
            claude_home: user_home.join(".claude"),
            grok_home: user_home.join(".grok"),
            pi_config: pi_config.clone(),
            other_homes: Vec::new(),
            claude_bin: None,
            grok_bin: None,
            runner: &runner,
        },
        codex_home: codex_home.clone(),
        codex_bin: Some(codex_bin),
    };

    let codex_config = codex_home.join("config.toml");
    let pi_settings = pi_config.join("settings.json");

    let available = list_available_plugins_with_v2(&apply, AgentId::Codex)?;
    let available_row = available
        .iter()
        .find(|row| {
            row.name == CODEX_NAME
                && row.marketplace.as_deref() == Some(CODEX_MARKET)
                && row.install_source.as_deref() == Some(CODEX_SOURCE)
        })
        .ok_or_else(|| "Codex fixture is not available".to_string())?;
    let available_listed = available_row
        .components
        .iter()
        .any(|component| component.kind == "skills" && component.name == "agenthub-probe");
    require(
        available_listed,
        "Codex fixture component was not discovered",
    )?;

    let preview = preview_plugin_install_with_v2(&apply, AgentId::Codex, CODEX_SOURCE)?;
    let codex_preview_matches = preview.name == CODEX_NAME
        && preview.marketplace.as_deref() == Some(CODEX_MARKET)
        && preview.install_source.as_deref() == Some(CODEX_SOURCE);
    require(
        codex_preview_matches,
        "Codex preview does not match the fixture",
    )?;

    let before = snapshot(&codex_config)?;
    let codex_confirmation_guarded = install_plugin_with_v2(
        &apply,
        AgentId::Codex,
        CODEX_SOURCE,
        PluginInstallOptions { confirmed: false },
    )
    .is_err()
        && snapshot(&codex_config)? == before;
    require(
        codex_confirmation_guarded,
        "Codex confirmation guard mutated config",
    )?;

    let codex_invalid_source_rejected = install_plugin_with_v2(
        &apply,
        AgentId::Codex,
        "bad;source@agenthub-probe-market",
        PluginInstallOptions { confirmed: true },
    )
    .is_err()
        && snapshot(&codex_config)? == before;
    require(
        codex_invalid_source_rejected,
        "Codex invalid source was not rejected",
    )?;

    install_plugin_with_v2(
        &apply,
        AgentId::Codex,
        CODEX_SOURCE,
        PluginInstallOptions { confirmed: true },
    )?;
    let inventory = list_plugin_inventory_with_v2(&scan);
    let codex_row = inventory
        .plugins
        .iter()
        .find(|row| row.agent == AgentId::Codex && row.name == CODEX_NAME)
        .ok_or_else(|| "installed Codex plugin is missing from inventory".to_string())?;
    let codex_installed_listed = codex_row.marketplace.as_deref() == Some(CODEX_MARKET)
        && codex_row.install_source.as_deref() == Some(CODEX_SOURCE)
        && codex_row.enabled == Some(true);
    require(
        codex_installed_listed,
        "installed Codex inventory row is incomplete",
    )?;

    disable_plugin_with_v2(&apply, AgentId::Codex, CODEX_NAME, Some(CODEX_MARKET))?;
    let disabled_listed = list_plugin_inventory_with_v2(&scan)
        .plugins
        .iter()
        .any(|row| {
            row.agent == AgentId::Codex && row.name == CODEX_NAME && row.enabled == Some(false)
        });
    require(disabled_listed, "Codex disabled state was not listed")?;

    enable_plugin_with_v2(&apply, AgentId::Codex, CODEX_NAME, Some(CODEX_MARKET))?;
    let enabled_listed = list_plugin_inventory_with_v2(&scan)
        .plugins
        .iter()
        .any(|row| {
            row.agent == AgentId::Codex && row.name == CODEX_NAME && row.enabled == Some(true)
        });
    require(enabled_listed, "Codex enabled state was not listed")?;

    let before_failed_install = snapshot(&codex_config)?;
    let codex_failed_install_rolled_back = install_plugin_with_v2(
        &apply,
        AgentId::Codex,
        "missing-probe@agenthub-probe-market",
        PluginInstallOptions { confirmed: true },
    )
    .is_err()
        && snapshot(&codex_config)? == before_failed_install;
    require(
        codex_failed_install_rolled_back,
        "failed Codex install did not restore config",
    )?;

    uninstall_plugin_with_v2(
        &apply,
        AgentId::Codex,
        CODEX_NAME,
        Some(CODEX_MARKET),
        Some(CODEX_SOURCE),
        PluginUninstallOptions::default(),
    )?;
    let codex_removed = !list_plugin_inventory_with_v2(&scan)
        .plugins
        .iter()
        .any(|row| row.agent == AgentId::Codex && row.name == CODEX_NAME);
    require(
        codex_removed,
        "Codex plugin remained in inventory after remove",
    )?;

    let before_relative = snapshot(&pi_settings)?;
    let relative_source = "./pi-package";
    let pi_relative_local_rejected =
        preview_plugin_install_with_v2(&apply, AgentId::Pi, relative_source).is_err()
            && install_plugin_with_v2(
                &apply,
                AgentId::Pi,
                relative_source,
                PluginInstallOptions { confirmed: true },
            )
            .is_err()
            && snapshot(&pi_settings)? == before_relative;
    require(
        pi_relative_local_rejected,
        "Pi relative local source was not rejected consistently",
    )?;

    let pi_source = pi_package.to_string_lossy().into_owned();
    let pi_preview = preview_plugin_install_with_v2(&apply, AgentId::Pi, &pi_source)?;
    let pi_preview_matches = pi_preview.install_source.as_deref() == Some(pi_source.as_str());
    require(pi_preview_matches, "Pi preview lost its install source")?;

    let before = snapshot(&pi_settings)?;
    let pi_confirmation_guarded = install_plugin_with_v2(
        &apply,
        AgentId::Pi,
        &pi_source,
        PluginInstallOptions { confirmed: false },
    )
    .is_err()
        && snapshot(&pi_settings)? == before;
    require(
        pi_confirmation_guarded,
        "Pi confirmation guard mutated settings",
    )?;

    let pi_invalid_source_rejected = install_plugin_with_v2(
        &apply,
        AgentId::Pi,
        "bad;source",
        PluginInstallOptions { confirmed: true },
    )
    .is_err()
        && snapshot(&pi_settings)? == before;
    require(
        pi_invalid_source_rejected,
        "Pi invalid source was not rejected",
    )?;

    install_plugin_with_v2(
        &apply,
        AgentId::Pi,
        &pi_source,
        PluginInstallOptions { confirmed: true },
    )?;
    let configured_source = configured_pi_source(&pi_settings)?;
    let inventory = list_plugin_inventory_with_v2(&scan);
    let pi_row = inventory
        .plugins
        .iter()
        .find(|row| row.agent == AgentId::Pi && row.name == PI_NAME)
        .ok_or_else(|| "installed Pi package is missing from inventory".to_string())?;
    let pi_installed_listed = pi_row
        .components
        .iter()
        .any(|component| component.kind == "skills" && component.name == "agenthub-probe");
    require(
        pi_installed_listed,
        "Pi package component was not discovered",
    )?;
    let inventory_source_exact = pi_row.install_source.as_deref() == Some(&configured_source);
    require(
        inventory_source_exact,
        "Pi inventory changed the configured source",
    )?;

    let before_toggle = snapshot(&pi_settings)?;
    let toggle_rejected = enable_plugin_with_v2(&apply, AgentId::Pi, PI_NAME, None).is_err()
        && disable_plugin_with_v2(&apply, AgentId::Pi, PI_NAME, None).is_err()
        && snapshot(&pi_settings)? == before_toggle;
    require(toggle_rejected, "Pi toggle was not rejected safely")?;

    let missing_source = pi_package.with_file_name("missing-pi-package");
    require(
        !missing_source.exists(),
        "missing Pi fixture unexpectedly exists",
    )?;
    let before_failed_install = snapshot(&pi_settings)?;
    let pi_failed_install_rolled_back = install_plugin_with_v2(
        &apply,
        AgentId::Pi,
        &missing_source.to_string_lossy(),
        PluginInstallOptions { confirmed: true },
    )
    .is_err()
        && snapshot(&pi_settings)? == before_failed_install;
    require(
        pi_failed_install_rolled_back,
        "failed Pi install did not restore settings",
    )?;

    uninstall_plugin_with_v2(
        &apply,
        AgentId::Pi,
        PI_NAME,
        Some("local"),
        Some(&configured_source),
        PluginUninstallOptions::default(),
    )?;
    let pi_removed = !list_plugin_inventory_with_v2(&scan)
        .plugins
        .iter()
        .any(|row| row.agent == AgentId::Pi && row.name == PI_NAME);
    require(pi_removed, "Pi package remained in inventory after remove")?;

    let evidence = ProbeEvidence {
        schema: "plugin-write-probe.v1",
        status: "ok",
        codex: CodexEvidence {
            available_listed,
            preview_matches: codex_preview_matches,
            confirmation_guarded: codex_confirmation_guarded,
            invalid_source_rejected: codex_invalid_source_rejected,
            installed_listed: codex_installed_listed,
            disabled_listed,
            enabled_listed,
            failed_install_rolled_back: codex_failed_install_rolled_back,
            removed: codex_removed,
        },
        pi: PiEvidence {
            preview_matches: pi_preview_matches,
            relative_local_rejected: pi_relative_local_rejected,
            confirmation_guarded: pi_confirmation_guarded,
            invalid_source_rejected: pi_invalid_source_rejected,
            installed_listed: pi_installed_listed,
            inventory_source_exact,
            toggle_rejected,
            failed_install_rolled_back: pi_failed_install_rolled_back,
            removed: pi_removed,
        },
    };
    println!(
        "{}",
        serde_json::to_string(&evidence)
            .map_err(|error| format!("serialize evidence failed: {error}"))?
    );
    Ok(())
}
