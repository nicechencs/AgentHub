use super::*;
use crate::models::RuntimeId;
use serde_json::json;
use std::sync::Mutex;

static PI_CONFIG_ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_pi_official_catalog<T>(models: Vec<String>, f: impl FnOnce() -> T) -> T {
    super::TEST_PI_OFFICIAL_CATALOG.with(|cell| {
        *cell.borrow_mut() = Some(models);
    });
    let result = f();
    super::TEST_PI_OFFICIAL_CATALOG.with(|cell| {
        *cell.borrow_mut() = None;
    });
    result
}

fn with_pi_config_dir<T>(f: impl FnOnce(&std::path::Path) -> T) -> T {
    let _guard = PI_CONFIG_ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("PI_CODING_AGENT_DIR");
    std::env::set_var("PI_CODING_AGENT_DIR", dir.path());
    let result = f(dir.path());
    match previous {
        Some(value) => std::env::set_var("PI_CODING_AGENT_DIR", value),
        None => std::env::remove_var("PI_CODING_AGENT_DIR"),
    }
    result
}

fn fake_node22() -> crate::runtime::ResolvedNode {
    crate::runtime::ResolvedNode {
        path: PathBuf::from("/tmp/mock-node-v22.19.0/bin/node"),
        version: "22.19.0".into(),
        major: 22,
    }
}

#[test]
fn build_run_spec_print_mode() {
    with_pi_config_dir(|_| {
        let bin = PathBuf::from("pi");
        let opts = RunOptions::default();
        let spec = build_pi_run_spec(&bin, "hello", &opts, Some(&fake_node22())).unwrap();
        assert_eq!(spec.agent, AgentId::Pi);
        assert_eq!(spec.program, bin);
        assert_eq!(spec.args[0], "-p");
        assert_eq!(spec.args[1], "hello");
        assert!(spec.args.iter().any(|a| a == "--mode"));
        assert!(spec.args.iter().any(|a| a == "text"));
        assert!(spec.args.iter().any(|a| a == "--no-session"));
        assert!(!spec.args.iter().any(|a| a == "--approve"));
        assert!(!spec.args.iter().any(|a| a == "--provider"));
        for (k, v) in &spec.env {
            assert_eq!(k, "PATH");
            assert!(!v.is_empty());
        }
    });
}

#[test]
fn build_run_spec_allow_dangerous_adds_approve() {
    with_pi_config_dir(|_| {
        let mut opts = RunOptions::default();
        opts.allow_dangerous = true;
        let spec = build_pi_run_spec(Path::new("pi"), "x", &opts, Some(&fake_node22())).unwrap();
        assert!(spec.args.iter().any(|a| a == "--approve"));
    });
}

#[test]
fn build_run_spec_uses_sole_auth_json_provider() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("auth.json"),
            serde_json::to_vec_pretty(&json!({
                "xai": { "type": "oauth", "access": "test-access" }
            }))
            .unwrap(),
        )
        .unwrap();
        let spec = build_pi_run_spec(
            Path::new("pi"),
            "ping",
            &RunOptions::default(),
            Some(&fake_node22()),
        )
        .unwrap();
        let args = spec.args;
        let idx = args
            .iter()
            .position(|a| a == "--provider")
            .expect("--provider");
        assert_eq!(args[idx + 1], "xai");
        assert!(!args.iter().any(|a| a == "--model"));
    });
}

#[test]
fn build_run_spec_disables_thinking_for_grok_code_fast() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "grok-code-fast-1",
                "defaultThinkingLevel": "low"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        let spec = build_pi_run_spec(
            Path::new("pi"),
            "ping",
            &RunOptions::default(),
            Some(&fake_node22()),
        )
        .unwrap();
        let think = spec
            .args
            .iter()
            .position(|a| a == "--thinking")
            .expect("--thinking");
        let dash_p = spec.args.iter().position(|a| a == "-p").expect("-p");
        assert_eq!(spec.args[think + 1], "off");
        assert!(
            think < dash_p,
            "--thinking must precede -p so Pi does not swallow it as the prompt: {:?}",
            spec.args
        );
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["defaultThinkingLevel"], "off");
    });
}

#[test]
fn build_run_spec_disables_thinking_when_catalog_also_has_grok_4() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "grok-code-fast-1",
                "defaultThinkingLevel": "low"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("models.json"),
            serde_json::to_vec_pretty(&json!({
                "providers": {
                    "xai": {
                        "models": [
                            { "id": "grok-4" },
                            { "id": "grok-code-fast-1" }
                        ]
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        let spec = build_pi_run_spec(
            Path::new("pi"),
            "ping",
            &RunOptions::default(),
            Some(&fake_node22()),
        )
        .unwrap();
        let think = spec
            .args
            .iter()
            .position(|a| a == "--thinking")
            .expect("--thinking must follow the send model, not the full catalog");
        assert_eq!(spec.args[think + 1], "off");
        assert!(think < spec.args.iter().position(|a| a == "-p").unwrap());
    });
}

#[test]
fn build_run_spec_keeps_thinking_for_other_models() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "grok-4",
                "defaultThinkingLevel": "low"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        let spec = build_pi_run_spec(
            Path::new("pi"),
            "ping",
            &RunOptions::default(),
            Some(&fake_node22()),
        )
        .unwrap();
        let think = spec
            .args
            .iter()
            .position(|a| a == "--thinking")
            .expect("--thinking");
        let dash_p = spec.args.iter().position(|a| a == "-p").expect("-p");
        assert_eq!(spec.args[think + 1], "low");
        assert!(think < dash_p, "{:?}", spec.args);
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["defaultThinkingLevel"], "low");
    });
}

#[test]
fn build_run_spec_uses_settings_default_provider() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({ "defaultProvider": "xai" })).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("auth.json"),
            serde_json::to_vec_pretty(&json!({
                "anthropic": { "type": "oauth", "access": "other-access" },
                "xai": { "type": "oauth", "access": "test-access" }
            }))
            .unwrap(),
        )
        .unwrap();
        let spec = build_pi_run_spec(
            Path::new("pi"),
            "ping",
            &RunOptions::default(),
            Some(&fake_node22()),
        )
        .unwrap();
        let args = spec.args;
        let idx = args
            .iter()
            .position(|a| a == "--provider")
            .expect("--provider");
        assert_eq!(args[idx + 1], "xai");
    });
}

#[test]
fn install_channels_npm_only() {
    let channels = PiAdapter.install_channels();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, "npm");
    assert!(channels[0].requires.contains(&RuntimeId::NodeJs));
    assert!(channels[0].requires.contains(&RuntimeId::Npm));
}

#[test]
fn skills_dir_is_under_agent_config() {
    let _guard = PI_CONFIG_ENV_LOCK.lock().unwrap();
    let dir = PiAdapter.skills_dir().expect("skills_dir");
    let expected = pi_config_dir().expect("pi_config_dir").join("skills");
    assert_eq!(dir, expected);
}

#[test]
fn set_pi_default_model_writes_settings_and_rejects_retired_backup() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "openrouter",
                "defaultModel": "stealth/ox-alpha"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("auth.json"),
            b"{\"openai\":{\"type\":\"oauth\"}}\n",
        )
        .unwrap();
        set_pi_default_model("openrouter/auto").unwrap();
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["defaultModel"], "openrouter/auto");
        assert_eq!(settings["defaultProvider"], "openrouter");
        let auth = std::fs::read_to_string(dir.join("auth.json")).unwrap();
        assert!(auth.contains("openai"), "{auth}");
        let err = set_pi_default_model("stealth/ox-alpha").unwrap_err();
        assert!(err.to_string().contains("下架"), "{err}");

        set_pi_default_model("grok-code-fast-1").unwrap();
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["defaultModel"], "grok-code-fast-1");
        assert_eq!(settings["defaultThinkingLevel"], "off");
    });
}

#[test]
fn set_pi_default_thinking_writes_settings_and_rejects_code_fast() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "grok-4"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        set_pi_default_thinking("high").unwrap();
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["defaultThinkingLevel"], "high");
        assert_eq!(settings["defaultModel"], "grok-4");
        let err = set_pi_default_thinking("turbo").unwrap_err();
        assert!(err.to_string().contains("不支持的思考等级"), "{err}");

        set_pi_default_model("grok-code-fast-1").unwrap();
        let err = set_pi_default_thinking("high").unwrap_err();
        assert!(err.to_string().contains("不支持思考等级"), "{err}");
        let live = pi_live_chat_model();
        assert!(live.efforts.is_empty(), "{live:?}");
        assert_eq!(live.effort, None);
    });
}

#[test]
fn pi_live_chat_model_exposes_thinking_levels() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "grok-4",
                "defaultThinkingLevel": "minimal"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        let live = pi_live_chat_model();
        assert_eq!(live.effort.as_deref(), Some("minimal"));
        assert_eq!(
            live.efforts,
            vec!["off", "minimal", "low", "medium", "high", "xhigh", "max"]
        );
    });
}

#[test]
fn pin_pi_live_slot_drops_bare_gpt_even_when_already_on_xai() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "gpt-5.5"
            }))
            .unwrap(),
        )
        .unwrap();
        pin_pi_live_slot("xai").unwrap();
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["defaultProvider"], "xai");
        assert!(settings.get("defaultModel").is_none(), "{settings}");
    });
}

#[test]
fn pin_pi_live_slot_keeps_gpt_on_openai_codex() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "openai-codex",
                "defaultModel": "gpt-5.5"
            }))
            .unwrap(),
        )
        .unwrap();
        pin_pi_live_slot("openai-codex").unwrap();
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["defaultModel"], "gpt-5.5");
    });
}

#[test]
fn reconcile_remaining_xai_replaces_gpt_with_catalog_model() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "openai-codex",
                "defaultModel": "gpt-5.5"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        with_pi_official_catalog(
            vec![
                "grok-4.3".into(),
                "grok-4.5".into(),
                "grok-4.6".into(),
                "grok-build-0.1".into(),
            ],
            || {
                reconcile_pi_default_for_remaining_slots(&["xai".into()]).unwrap();
                let settings: serde_json::Value = serde_json::from_str(
                    &std::fs::read_to_string(dir.join("settings.json")).unwrap(),
                )
                .unwrap();
                assert_eq!(settings["defaultProvider"], "xai");
                assert_eq!(settings["defaultModel"], "grok-4.6");
            },
        );
    });
}

#[test]
fn pi_cli_provider_args_skips_foreign_gpt_on_xai() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "gpt-5.5"
            }))
            .unwrap(),
        )
        .unwrap();
        let spec = build_pi_run_spec(
            Path::new("pi"),
            "ping",
            &RunOptions::default(),
            Some(&fake_node22()),
        )
        .unwrap();
        let idx = spec
            .args
            .iter()
            .position(|a| a == "--provider")
            .expect("--provider");
        assert_eq!(spec.args[idx + 1], "xai");
        assert!(!spec.args.iter().any(|a| a == "--model"), "{:?}", spec.args);
    });
}

#[test]
fn pi_live_chat_model_drops_foreign_gpt_without_inventing() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "gpt-5.5"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("models.json"), b"{}\n").unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        let live = pi_live_chat_model();
        assert_eq!(live.model, None);
        assert!(live.models.is_empty(), "{live:?}");
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert!(settings.get("defaultModel").is_none(), "{settings}");
        assert_eq!(settings["defaultProvider"], "xai");
    });
}

#[test]
fn pi_live_chat_model_reads_current_slot_and_skips_leftover_url_catalog() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "grok-4"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("models.json"),
            serde_json::to_vec_pretty(&json!({
                "providers": {
                    "openrouter": {
                        "baseUrl": "https://openrouter.ai/api/v1",
                        "models": [{ "id": "openrouter/auto" }]
                    },
                    "xai": {
                        "models": [
                            { "id": "grok-4" },
                            { "id": "grok-code-fast-1" },
                            { "id": "stealth/ox-alpha" }
                        ]
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        let live = pi_live_chat_model();
        assert_eq!(live.model.as_deref(), Some("grok-4"));
        assert_eq!(
            live.models,
            vec!["grok-4".to_string(), "grok-code-fast-1".to_string()]
        );
        assert!(!live.models.iter().any(|id| id.contains("openrouter")));
        assert!(!live.models.iter().any(|id| id.contains("stealth")));
    });
}

#[test]
fn pi_live_chat_model_has_no_hardcoded_grok_fallback() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({ "defaultProvider": "xai" })).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("models.json"), b"{}\n").unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        let live = pi_live_chat_model();
        assert_eq!(live.model, None);
        assert!(live.models.is_empty(), "{live:?}");
    });
}

#[test]
fn parse_pi_list_models_keeps_only_the_login_slot() {
    let stdout = "\
provider model            context max-out thinking images
xai      grok-4.3         2M      64K     yes      yes
xai      grok-4.5         256K    64K     yes      yes
xai      grok-4.6         256K    64K     yes      yes
xai      grok-build-0.1   128K    16K     no       no
openrouter openrouter/auto 200K   16K     no       no
";
    assert_eq!(
        parse_pi_list_models_output(stdout, "xai"),
        vec![
            "grok-4.3".to_string(),
            "grok-4.5".to_string(),
            "grok-4.6".to_string(),
            "grok-build-0.1".to_string()
        ]
    );
    assert!(parse_pi_list_models_output(stdout, "xai")
        .iter()
        .all(|id| !id.contains("openrouter")));
}

#[test]
fn pi_live_chat_model_uses_official_catalog_and_replaces_leftover_default() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "grok-code-fast-1"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("models.json"), b"{}\n").unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        with_pi_official_catalog(
            vec![
                "grok-4.3".into(),
                "grok-4.5".into(),
                "grok-4.6".into(),
                "grok-build-0.1".into(),
            ],
            || {
                let live = pi_live_chat_model();
                assert_eq!(
                    live.models,
                    vec![
                        "grok-4.3".to_string(),
                        "grok-4.5".to_string(),
                        "grok-4.6".to_string(),
                        "grok-build-0.1".to_string()
                    ]
                );
                assert_eq!(live.model.as_deref(), Some("grok-4.6"));
                assert!(!live.models.iter().any(|id| id.contains("grok-code-fast")));
                let settings: serde_json::Value = serde_json::from_str(
                    &std::fs::read_to_string(dir.join("settings.json")).unwrap(),
                )
                .unwrap();
                assert_eq!(settings["defaultModel"], "grok-4.6");
            },
        );
    });
}

#[test]
fn pi_live_chat_model_does_not_skip_remote_when_models_json_has_leftover() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "grok-code-fast-1"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("models.json"),
            serde_json::to_vec_pretty(&json!({
                "providers": {
                    "xai": {
                        "models": [{ "id": "grok-code-fast-1" }]
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        with_pi_official_catalog(
            vec![
                "grok-4.3".into(),
                "grok-4.5".into(),
                "grok-4.6".into(),
                "grok-build-0.1".into(),
            ],
            || {
                let live = pi_live_chat_model();
                assert_eq!(
                    live.models,
                    vec![
                        "grok-4.3".to_string(),
                        "grok-4.5".to_string(),
                        "grok-4.6".to_string(),
                        "grok-build-0.1".to_string()
                    ]
                );
                assert_eq!(live.model.as_deref(), Some("grok-4.6"));
                assert!(!live.models.iter().any(|id| id.contains("grok-code-fast")));
            },
        );
    });
}

#[test]
fn pi_live_chat_model_keeps_catalog_current_without_rewriting() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "xai",
                "defaultModel": "grok-4.6"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("models.json"), b"{}\n").unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        with_pi_official_catalog(
            vec![
                "grok-4.3".into(),
                "grok-4.5".into(),
                "grok-4.6".into(),
                "grok-build-0.1".into(),
            ],
            || {
                let live = pi_live_chat_model();
                assert_eq!(live.model.as_deref(), Some("grok-4.6"));
                assert_eq!(
                    live.models,
                    vec![
                        "grok-4.3".to_string(),
                        "grok-4.5".to_string(),
                        "grok-4.6".to_string(),
                        "grok-build-0.1".to_string()
                    ]
                );
            },
        );
    });
}

#[test]
fn apply_oauth_pins_slot_and_drops_leftover_stealth_model() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "defaultProvider": "openrouter",
                "defaultModel": "stealth/ox-alpha"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("auth.json"), b"{}\n").unwrap();
        PiAdapter
            .apply_account(&LiveAccount {
                agent: AgentId::Pi,
                kind: crate::models::AccountKind::Oauth,
                credentials: json!({
                    "format": "auth_json",
                    "provider": "xai",
                    "body": {
                        "xai": { "type": "oauth", "access": "at-xai", "refresh": "rt-xai" }
                    }
                }),
                label_hint: Some("pi:xai".into()),
                extra: json!({ "provider": "xai" }),
            })
            .unwrap();
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["defaultProvider"], "xai");
        assert!(settings.get("defaultModel").is_none(), "{settings}");
        let spec = build_pi_run_spec(
            Path::new("pi"),
            "ping",
            &RunOptions::default(),
            Some(&fake_node22()),
        )
        .unwrap();
        let idx = spec
            .args
            .iter()
            .position(|a| a == "--provider")
            .expect("--provider");
        assert_eq!(spec.args[idx + 1], "xai");
        assert!(!spec.args.iter().any(|a| a == "--model"));
    });
}

#[test]
fn live_backup_paths_include_settings_and_auth() {
    let paths = PiAdapter.live_backup_paths();
    assert!(!paths.is_empty());
    let joined: Vec<String> = paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(joined.iter().any(|n| n == "settings.json"));
    assert!(joined.iter().any(|n| n == "auth.json"));
    assert!(joined.iter().any(|n| n == "models.json"));
    assert!(joined.iter().any(|n| n == "mcp.json"));
}

#[test]
fn merge_models_preserves_unrelated_providers_and_redacted_keys() {
    let live = json!({
        "providers": {
            "keep": { "baseUrl": "https://keep", "apiKey": "live-secret", "unknown": 1 },
            "custom": { "baseUrl": "https://old", "apiKey": "old-secret" }
        },
        "unknownTopLevel": true
    });
    let desired = json!({
        "providers": {
            "custom": { "baseUrl": "https://new", "apiKey": "***" }
        }
    });
    let merged = merge_pi_models(&live, &desired).unwrap();
    assert_eq!(merged["providers"]["keep"]["apiKey"], "live-secret");
    assert_eq!(merged["providers"]["keep"]["unknown"], 1);
    assert_eq!(merged["providers"]["custom"]["baseUrl"], "https://new");
    assert_eq!(merged["providers"]["custom"]["apiKey"], "old-secret");
    assert_eq!(merged["unknownTopLevel"], true);
}

#[test]
fn merge_models_requires_provider_object() {
    let err = merge_pi_models(&json!({}), &json!({"models": []})).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn write_config_auth_only_merges_and_snapshot_auth_replaces() {
    let _guard = PI_CONFIG_ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("PI_CODING_AGENT_DIR");
    std::env::set_var("PI_CODING_AGENT_DIR", dir.path());
    std::fs::write(
        dir.path().join("auth.json"),
        serde_json::to_vec_pretty(&json!({
            "keep": { "type": "oauth", "access": "keep-access" },
            "anthropic": { "type": "oauth", "access": "old-access", "refresh": "old-refresh" }
        }))
        .unwrap(),
    )
    .unwrap();

    write_pi_config(&AgentConfig {
        agent: AgentId::Pi,
        raw: json!({
            "auth": {
                "anthropic": {
                    "type": "oauth",
                    "access": "new-access",
                    "refresh": "new-refresh"
                },
                "keep": { "type": "oauth", "access": "***" }
            }
        }),
    })
    .unwrap();
    let merged = read_json_object_or_empty(&dir.path().join("auth.json")).unwrap();
    assert_eq!(merged["keep"]["access"], "keep-access");
    assert_eq!(merged["anthropic"]["access"], "new-access");

    write_pi_config(&AgentConfig {
        agent: AgentId::Pi,
        raw: json!({
            "auth": {
                "only": { "type": "oauth", "access": "snapshot-access" }
            },
            "paths": { "auth": "snapshot.json" }
        }),
    })
    .unwrap();
    let replaced = read_json_object_or_empty(&dir.path().join("auth.json")).unwrap();
    assert_eq!(
        replaced,
        json!({
            "only": { "type": "oauth", "access": "snapshot-access" }
        })
    );

    match previous {
        Some(value) => std::env::set_var("PI_CODING_AGENT_DIR", value),
        None => std::env::remove_var("PI_CODING_AGENT_DIR"),
    }
}

#[test]
fn write_config_models_and_auth_do_not_cross_files() {
    let _guard = PI_CONFIG_ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("PI_CODING_AGENT_DIR");
    std::env::set_var("PI_CODING_AGENT_DIR", dir.path());
    std::fs::write(
        dir.path().join("auth.json"),
        serde_json::to_vec_pretty(&json!({
            "keep": { "type": "oauth", "access": "keep-access" }
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("models.json"),
        serde_json::to_vec_pretty(&json!({
            "providers": {
                "keep": { "baseUrl": "https://keep.example", "apiKey": "keep-secret" }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    write_pi_config(&AgentConfig {
        agent: AgentId::Pi,
        raw: json!({
            "models": {
                "providers": {
                    "custom": {
                        "baseUrl": "https://relay.example/v1",
                        "api": "openai-completions",
                        "apiKey": "sk-relay",
                        "models": [{ "id": "custom-model" }]
                    }
                }
            },
            "auth": {
                "openai": { "type": "api_key", "key": "sk-openai" }
            }
        }),
    })
    .unwrap();

    let models = read_json_object_or_empty(&dir.path().join("models.json")).unwrap();
    assert_eq!(
        models["providers"]["custom"]["baseUrl"],
        "https://relay.example/v1"
    );
    assert_eq!(models["providers"]["keep"]["apiKey"], "keep-secret");
    assert!(
        models.get("auth").is_none(),
        "auth must not leak into models.json"
    );

    let auth = read_json_object_or_empty(&dir.path().join("auth.json")).unwrap();
    assert_eq!(auth["openai"]["type"], "api_key");
    assert_eq!(auth["openai"]["key"], "sk-openai");
    assert_eq!(auth["keep"]["access"], "keep-access");

    // Legacy root `{ providers, auth }` must also keep auth out of models.json.
    write_pi_config(&AgentConfig {
        agent: AgentId::Pi,
        raw: json!({
            "providers": {
                "extra": { "baseUrl": "https://extra.example", "apiKey": "sk-extra" }
            },
            "auth": {
                "deepseek": { "type": "api_key", "key": "sk-ds" }
            }
        }),
    })
    .unwrap();
    let models = read_json_object_or_empty(&dir.path().join("models.json")).unwrap();
    assert_eq!(
        models["providers"]["extra"]["baseUrl"],
        "https://extra.example"
    );
    assert!(models.get("auth").is_none());
    let auth = read_json_object_or_empty(&dir.path().join("auth.json")).unwrap();
    assert_eq!(auth["deepseek"]["key"], "sk-ds");
    assert_eq!(auth["openai"]["key"], "sk-openai");

    match previous {
        Some(value) => std::env::set_var("PI_CODING_AGENT_DIR", value),
        None => std::env::remove_var("PI_CODING_AGENT_DIR"),
    }
}

#[test]
fn write_config_settings_default_provider_merges_without_clobber() {
    with_pi_config_dir(|dir| {
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec_pretty(&json!({
                "theme": "dark",
                "defaultThinkingLevel": "low"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("auth.json"),
            serde_json::to_vec_pretty(&json!({
                "keep": { "type": "oauth", "access": "keep-access" }
            }))
            .unwrap(),
        )
        .unwrap();

        write_pi_config(&AgentConfig {
            agent: AgentId::Pi,
            raw: json!({
                "settings": { "defaultProvider": "xai" },
                "auth": {
                    "xai": { "type": "oauth", "access": "new-access" }
                }
            }),
        })
        .unwrap();

        let settings = read_json_object_or_empty(&dir.join("settings.json")).unwrap();
        assert_eq!(settings["defaultProvider"], "xai");
        assert_eq!(settings["theme"], "dark");
        assert_eq!(settings["defaultThinkingLevel"], "low");

        let auth = read_json_object_or_empty(&dir.join("auth.json")).unwrap();
        assert_eq!(auth["xai"]["access"], "new-access");
        assert_eq!(auth["keep"]["access"], "keep-access");
    });
}

#[test]
fn pi_child_env_prefixes_node22_bin_on_path() {
    let dir = Path::new("/tmp/mock-node-v22.19.0/bin");
    let env = pi_child_env(Some(dir));
    assert_eq!(env.len(), 1);
    assert_eq!(env[0].0, "PATH");
    let prefixed = &env[0].1;
    #[cfg(windows)]
    {
        assert!(
            prefixed.starts_with(r"/tmp/mock-node-v22.19.0/bin;")
                || prefixed == r"/tmp/mock-node-v22.19.0/bin",
            "PATH must start with Node 22 bin: {prefixed}"
        );
    }
    #[cfg(not(windows))]
    {
        assert!(
            prefixed.starts_with("/tmp/mock-node-v22.19.0/bin:")
                || prefixed == "/tmp/mock-node-v22.19.0/bin",
            "PATH must start with Node 22 bin: {prefixed}"
        );
    }
}

#[test]
fn pi_child_env_empty_without_node22() {
    assert!(pi_child_env(None).is_empty());
}

#[test]
fn require_pi_node22_env_none_is_env_not_ready() {
    let err = require_pi_node22_env(None).unwrap_err();
    assert_eq!(err.code(), "env.not_ready");
    assert!(err.to_string().contains("Node too old"), "message={err}");
}

#[test]
fn require_pi_node22_env_prefixes_bin_dir() {
    let node = fake_node22();
    let env = require_pi_node22_env(Some(&node)).unwrap();
    assert_eq!(env, pi_child_env(node.bin_dir().as_deref()));
    assert_eq!(env.len(), 1);
    assert_eq!(env[0].0, "PATH");
    let bin = node.bin_dir().expect("fake node has a bin dir");
    let prefix = bin.to_string_lossy();
    assert!(
        env[0].1 == prefix || env[0].1.starts_with(prefix.as_ref()),
        "PATH must start with Node 22 bin: {}",
        env[0].1
    );
}

#[test]
fn build_pi_run_spec_without_node22_is_env_not_ready() {
    with_pi_config_dir(|_| {
        let err =
            build_pi_run_spec(Path::new("pi"), "hello", &RunOptions::default(), None).unwrap_err();
        assert_eq!(err.code(), "env.not_ready");
        assert!(err.to_string().contains("Node too old"), "message={err}");
    });
}

#[test]
fn apply_pi_node_requirement_marks_env_not_ready_and_node_too_old() {
    let detect = DetectResult {
        agent: AgentId::Pi,
        status: crate::models::DetectStatus::Installed,
        version: None,
        binary_path: Some(PathBuf::from("/usr/bin/pi")),
        channel: Some("npm".into()),
        env_ready: true,
        notes: vec![],
        extra_copies: Vec::new(),
    };
    let out = apply_pi_node_requirement(detect, false);
    assert!(!out.env_ready, "env must not be ready without Node 22");
    assert!(
        out.notes.iter().any(|n| n.contains("Node too old")),
        "notes={:?}",
        out.notes
    );
    assert!(!out
        .notes
        .iter()
        .any(|n| n.contains("已安装但未读到本机版本号")));
}

#[test]
fn apply_pi_node_requirement_keeps_ready_when_node22_present() {
    let detect = DetectResult {
        agent: AgentId::Pi,
        status: crate::models::DetectStatus::Installed,
        version: Some("0.83.0".into()),
        binary_path: Some(PathBuf::from("/usr/bin/pi")),
        channel: Some("npm".into()),
        env_ready: true,
        notes: vec![],
        extra_copies: Vec::new(),
    };
    let out = apply_pi_node_requirement(detect, true);
    assert!(out.env_ready);
    assert!(out.notes.is_empty());
}
