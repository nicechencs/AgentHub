use super::*;
use serde_json::json;

#[test]
fn rejects_unknown_model_when_catalog_present() {
    let catalog = vec![RuntimeModelOption {
        id: "gpt-test".into(),
        efforts: vec!["low".into(), "high".into()],
        default_effort: Some("low".into()),
    }];
    let err = validate_turn_settings(
        &RuntimeTurnSettings {
            model: Some("nope".into()),
            effort: None,
        },
        &catalog,
        &RuntimeTurnSettings::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("模型不可用"));
}

#[test]
fn rejects_effort_not_in_model_list() {
    let catalog = vec![RuntimeModelOption {
        id: "gpt-test".into(),
        efforts: vec!["low".into()],
        default_effort: Some("low".into()),
    }];
    let err = validate_turn_settings(
        &RuntimeTurnSettings {
            model: Some("gpt-test".into()),
            effort: Some("ultra".into()),
        },
        &catalog,
        &RuntimeTurnSettings::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("不支持思考强度"));
}

#[test]
fn default_turn_settings_uses_first_catalog_model() {
    let catalog = vec![
        RuntimeModelOption {
            id: "gpt-first".into(),
            efforts: vec!["low".into(), "high".into()],
            default_effort: Some("high".into()),
        },
        RuntimeModelOption {
            id: "gpt-second".into(),
            efforts: vec!["low".into()],
            default_effort: Some("low".into()),
        },
    ];
    let defaults = default_turn_settings(&catalog).unwrap();
    assert_eq!(defaults.model.as_deref(), Some("gpt-first"));
    assert_eq!(defaults.effort.as_deref(), Some("high"));
    assert!(default_turn_settings(&[]).is_none());
}

#[test]
fn fills_default_effort_when_omitted() {
    let catalog = vec![RuntimeModelOption {
        id: "gpt-test".into(),
        efforts: vec!["low".into(), "high".into()],
        default_effort: Some("high".into()),
    }];
    let ok = validate_turn_settings(
        &RuntimeTurnSettings {
            model: Some("gpt-test".into()),
            effort: None,
        },
        &catalog,
        &RuntimeTurnSettings::default(),
    )
    .unwrap();
    assert_eq!(ok.effort.as_deref(), Some("high"));
}

#[test]
fn fills_first_effort_when_default_missing_or_invalid() {
    let catalog = vec![RuntimeModelOption {
        id: "spark".into(),
        efforts: vec!["low".into(), "high".into()],
        default_effort: Some("medium".into()),
    }];
    let ok = validate_turn_settings(
        &RuntimeTurnSettings {
            model: Some("spark".into()),
            effort: None,
        },
        &catalog,
        &RuntimeTurnSettings::default(),
    )
    .unwrap();
    assert_eq!(ok.effort.as_deref(), Some("low"));
    assert_eq!(resolved_default_effort(&catalog[0]).as_deref(), Some("low"));
}

#[test]
fn reconcile_resets_unsupported_effort_for_model() {
    let catalog = vec![RuntimeModelOption {
        id: "gpt-5.3-codex-spark".into(),
        efforts: vec!["low".into(), "high".into()],
        default_effort: Some("low".into()),
    }];
    let repaired = reconcile_turn_settings(
        &RuntimeTurnSettings {
            model: Some("gpt-5.3-codex-spark".into()),
            effort: Some("medium".into()),
        },
        &catalog,
    )
    .expect("should repair");
    assert_eq!(repaired.model.as_deref(), Some("gpt-5.3-codex-spark"));
    assert_eq!(repaired.effort.as_deref(), Some("low"));
    assert!(reconcile_turn_settings(&repaired, &catalog).is_none());
}

#[test]
fn assert_settings_supported_rejects_bad_pair() {
    let catalog = vec![RuntimeModelOption {
        id: "gpt-5.3-codex-spark".into(),
        efforts: vec!["low".into()],
        default_effort: Some("low".into()),
    }];
    let err = assert_settings_supported(
        &RuntimeTurnSettings {
            model: Some("gpt-5.3-codex-spark".into()),
            effort: Some("medium".into()),
        },
        &catalog,
    )
    .unwrap_err();
    assert!(err.to_string().contains("不支持思考强度"));
    assert!(assert_settings_supported(
        &RuntimeTurnSettings {
            model: Some("gpt-5.3-codex-spark".into()),
            effort: None,
        },
        &catalog,
    )
    .is_ok());
}

#[test]
fn parse_model_list_drops_unsupported_default_effort() {
    let value = json!({
        "data": [{
            "id": "spark",
            "supportedReasoningEfforts": ["low", "high"],
            "defaultReasoningEffort": "medium"
        }]
    });
    let models = parse_model_list(&value);
    assert_eq!(models[0].efforts, vec!["low", "high"]);
    assert_eq!(models[0].default_effort.as_deref(), Some("low"));
}

#[test]
fn parse_model_list_accepts_models_key() {
    let value = json!({
        "models": [{
            "id": "gpt-reserve",
            "supportedReasoningEfforts": ["low", "high"],
            "defaultReasoningEffort": "high"
        }]
    });
    let models = parse_model_list(&value);
    assert_eq!(models[0].id, "gpt-reserve");
    assert_eq!(models[0].default_effort.as_deref(), Some("high"));
}

#[test]
fn parses_model_list_efforts() {
    let value = json!({
        "data": [{
            "id": "m1",
            "supportedReasoningEfforts": [{"reasoningEffort": "low"}, "high"],
            "defaultReasoningEffort": "low"
        }]
    });
    let models = parse_model_list(&value);
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].efforts, vec!["low", "high"]);
    assert_eq!(models[0].default_effort.as_deref(), Some("low"));
}

#[test]
fn parse_model_list_matches_live_codex_spark_object_shape() {
    // Codex 0.150+/0.153+ app-server list: objects with reasoningEffort + description.
    // Live catalog over-reports medium/xhigh for spark even though turn/start may reject some.
    let value = json!({
        "data": [{
            "id": "gpt-5.3-codex-spark",
            "displayName": "GPT-5.3-Codex-Spark",
            "supportedReasoningEfforts": [
                {"reasoningEffort": "low", "description": "Fast responses with lighter reasoning"},
                {"reasoningEffort": "medium", "description": "Balances speed and reasoning depth for everyday tasks"},
                {"reasoningEffort": "high", "description": "Greater reasoning depth for complex problems"},
                {"reasoningEffort": "xhigh", "description": "Extra high reasoning depth for complex problems"},
                {"effort": "medium", "available": false, "description": "duplicate marked unavailable"}
            ],
            "defaultReasoningEffort": "high"
        }]
    });
    let models = parse_model_list(&value);
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].efforts, vec!["low", "medium", "high", "xhigh"]);
    assert_eq!(models[0].default_effort.as_deref(), Some("high"));
}

#[test]
fn apply_denied_efforts_filters_over_reported_catalog() {
    let models = vec![RuntimeModelOption {
        id: "gpt-5.3-codex-spark".into(),
        efforts: vec!["low".into(), "medium".into(), "high".into(), "xhigh".into()],
        default_effort: Some("high".into()),
    }];
    let mut denied = std::collections::HashMap::new();
    denied.insert(
        "gpt-5.3-codex-spark".into(),
        ["medium".into()].into_iter().collect(),
    );
    let filtered = apply_denied_efforts(&models, &denied);
    assert_eq!(filtered[0].efforts, vec!["low", "high", "xhigh"]);
    assert_eq!(filtered[0].default_effort.as_deref(), Some("high"));

    let repaired = reconcile_turn_settings(
        &RuntimeTurnSettings {
            model: Some("gpt-5.3-codex-spark".into()),
            effort: Some("medium".into()),
        },
        &filtered,
    )
    .expect("should repair after deny");
    assert_eq!(repaired.effort.as_deref(), Some("high"));
}

#[test]
fn looks_like_thinking_unsupported_matches_localized_and_upstream() {
    assert!(looks_like_thinking_unsupported(
        "OpenAI API error: does not support parameter reasoningEffort"
    ));
    assert!(looks_like_thinking_unsupported(
        "这个模型不支持当前思考设置。请点重试。"
    ));
    assert!(!looks_like_thinking_unsupported("network timeout"));
}

#[test]
fn build_input_uses_local_image_not_path_text() {
    let input = build_turn_input(
        "see this",
        &[RuntimeLocalImage {
            path: "/tmp/a.png".into(),
        }],
        &[],
    )
    .unwrap();
    assert_eq!(input[0]["type"], "text");
    assert_eq!(input[1]["type"], "localImage");
    assert_eq!(input[1]["path"], "/tmp/a.png");
}

#[test]
fn rejects_unknown_skill_when_catalog_has_skills() {
    let catalog = vec![RuntimeExtensionItem {
        id: "/skills/demo/SKILL.md".into(),
        name: "demo".into(),
        kind: RuntimeExtensionKind::Skill,
        installed: true,
        enabled: true,
        loaded: false,
        callable: true,
        path: Some("/skills/demo/SKILL.md".into()),
    }];
    let err = validate_skill_refs(
        &[RuntimeSkillRef {
            name: "demo".into(),
            path: "/skills/other/SKILL.md".into(),
        }],
        &catalog,
    )
    .unwrap_err();
    assert!(err.to_string().contains("不可用于本轮"));
}

#[test]
fn may_fetch_catalog_only_when_idle() {
    use super::super::types::RuntimePhase;
    assert!(may_fetch_catalog(None));
    assert!(may_fetch_catalog(Some(RuntimePhase::Idle)));
    assert!(may_fetch_catalog(Some(RuntimePhase::Completed)));
    assert!(!may_fetch_catalog(Some(RuntimePhase::Starting)));
    assert!(!may_fetch_catalog(Some(RuntimePhase::Running)));
    assert!(!may_fetch_catalog(Some(RuntimePhase::Waiting)));
    assert!(!may_fetch_catalog(Some(RuntimePhase::Cancelling)));
}

#[test]
fn parse_grok_model_list_reads_nested_result_and_effort_objects() {
    let value = json!({
        "result": {
            "currentModelId": "grok-4.6",
            "availableModels": [{
                "modelId": "grok-4.6",
                "_meta": {
                    "reasoningEffort": "high",
                    "reasoningEfforts": [
                        { "id": "xhigh", "value": "xhigh", "default": false },
                        { "id": "high", "value": "high", "default": true },
                        { "id": "low", "value": "low", "default": false }
                    ]
                }
            }]
        }
    });
    let models = parse_grok_model_list(&value);
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "grok-4.6");
    assert_eq!(models[0].efforts, vec!["xhigh", "high", "low"]);
    assert_eq!(models[0].default_effort.as_deref(), Some("high"));
}

#[test]
fn ensure_grok_catalog_efforts_fills_defaults_and_clears_code_fast() {
    let models = ensure_grok_catalog_efforts(vec![
        RuntimeModelOption {
            id: "grok-4.6".into(),
            efforts: Vec::new(),
            default_effort: None,
        },
        RuntimeModelOption {
            id: "grok-code-fast-1".into(),
            efforts: vec!["high".into()],
            default_effort: Some("high".into()),
        },
    ]);
    assert_eq!(models[0].efforts, vec!["low", "high", "xhigh"]);
    assert_eq!(models[0].default_effort.as_deref(), Some("high"));
    assert!(models[1].efforts.is_empty());
    assert_eq!(models[1].default_effort, None);
}

#[test]
fn acp_session_plan_reuses_live_kiro_and_skips_cross_process_load() {
    assert_eq!(
        acp_session_plan(AgentId::Kiro, true, true),
        AcpSessionPlan::PromptExisting
    );
    assert_eq!(
        acp_session_plan(AgentId::Kiro, false, true),
        AcpSessionPlan::Unavailable
    );
    assert_eq!(
        acp_session_plan(AgentId::Kiro, false, false),
        AcpSessionPlan::New
    );
    assert_eq!(
        acp_session_plan(AgentId::Grok, true, true),
        AcpSessionPlan::PromptExisting
    );
    assert_eq!(
        acp_session_plan(AgentId::Grok, false, true),
        AcpSessionPlan::LoadThenPrompt
    );
    assert_eq!(
        acp_session_plan(AgentId::Grok, false, false),
        AcpSessionPlan::New
    );
}

#[test]
fn acp_session_prompt_params_use_prompt_not_content() {
    let blocks = grok_prompt_blocks("ping", &[]).unwrap();
    let params = acp_session_prompt_params("sess-1", blocks);
    assert_eq!(params["sessionId"], "sess-1");
    assert_eq!(params["prompt"][0]["text"], "ping");
    assert!(params.get("content").is_none());
}

#[test]
fn codex_workspace_write_excludes_tmp_so_outside_cwd_needs_approval() {
    let cwd = std::path::Path::new("/workspace/project");
    let policy = codex_workspace_write_sandbox_policy(cwd);
    assert_eq!(policy["type"], "workspaceWrite");
    assert_eq!(policy["writableRoots"], json!(["/workspace/project"]));
    assert_eq!(policy["networkAccess"], false);
    assert_eq!(policy["excludeSlashTmp"], true);
    assert_eq!(policy["excludeTmpdirEnvVar"], true);
}

#[test]
fn grok_acp_stdio_uses_documented_agent_flags_only() {
    assert_eq!(
        grok_acp_stdio_args(None, None, false),
        vec![
            "agent".to_string(),
            "--no-leader".to_string(),
            "stdio".to_string()
        ]
    );
    assert!(!grok_acp_stdio_args(None, None, false)
        .iter()
        .any(|arg| arg == "--permission-mode"));
    assert_eq!(
        grok_acp_stdio_args(Some("grok-4.6"), Some("high"), true),
        vec![
            "agent".to_string(),
            "--no-leader".to_string(),
            "-m".to_string(),
            "grok-4.6".to_string(),
            "--reasoning-effort".to_string(),
            "high".to_string(),
            "--always-approve".to_string(),
            "stdio".to_string()
        ]
    );
}

#[test]
fn grok_initialize_advertises_client_fs_without_terminal() {
    let params = grok_initialize_params();
    assert_eq!(params["protocolVersion"], 1);
    assert_eq!(params["clientCapabilities"]["fs"]["readTextFile"], true);
    assert_eq!(params["clientCapabilities"]["fs"]["writeTextFile"], true);
    assert_eq!(params["clientCapabilities"]["terminal"], false);
}

#[test]
fn path_is_inside_cwd_uses_real_directories() {
    let cwd = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let inside_new = cwd.path().join("new-file.txt");
    let outside_new = outside.path().join("out.txt");
    assert!(path_is_inside_cwd(&inside_new, cwd.path()));
    assert!(path_is_inside_cwd(
        std::path::Path::new("relative.txt"),
        cwd.path()
    ));
    assert!(!path_is_inside_cwd(&outside_new, cwd.path()));
}

#[test]
fn acp_fs_write_payload_requires_path_and_caps_size() {
    let (path, content) = acp_fs_write_payload(&json!({
        "path": "/tmp/agenthub-always-allow-grok-347.txt",
        "content": "hello"
    }))
    .unwrap();
    assert_eq!(
        path,
        std::path::PathBuf::from("/tmp/agenthub-always-allow-grok-347.txt")
    );
    assert_eq!(content, "hello");
    assert!(acp_fs_write_payload(&json!({"content": "x"})).is_err());
    let too_big = "x".repeat(ACP_FS_WRITE_MAX_BYTES + 1);
    assert!(acp_fs_write_payload(&json!({"path": "/tmp/x", "content": too_big})).is_err());
}

#[test]
fn grok_session_new_disables_yolo_unless_conversation_skips_cards() {
    let cwd = std::path::Path::new("/workspace/project");
    let ask = grok_session_new_params(cwd, false);
    assert_eq!(ask["cwd"], "/workspace/project");
    assert_eq!(ask["_meta"]["yoloMode"], false);
    assert_eq!(ask["_meta"]["autoMode"], false);
    let skip = grok_session_new_params(cwd, true);
    assert_eq!(skip["_meta"]["yoloMode"], true);
    assert!(skip["_meta"].get("autoMode").is_none());
}

#[test]
fn grok_prompt_blocks_embed_local_image() {
    // Grok/Kiro ACP image blocks require base64 `data`. A path-only / file URI
    // block is not sent even when initialize advertises client fs.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shot.png");
    std::fs::write(&path, b"png-bytes").unwrap();
    let blocks = grok_prompt_blocks(
        "look",
        &[RuntimeLocalImage {
            path: path.to_string_lossy().into_owned(),
        }],
    )
    .unwrap();
    assert_eq!(blocks[0]["type"], "text");
    assert_eq!(blocks[0]["text"], "look");
    assert_eq!(blocks[1]["type"], "image");
    assert_eq!(blocks[1]["mimeType"], "image/png");
    use base64::Engine;
    assert_eq!(
        blocks[1]["data"],
        base64::engine::general_purpose::STANDARD.encode(b"png-bytes")
    );
}

#[test]
fn claude_user_message_embeds_base64_image() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shot.png");
    // Minimal PNG header is enough for mime sniffing by extension.
    std::fs::write(&path, b"\x89PNG\r\n\x1a\nfake").unwrap();
    let message = claude_user_message(
        "what color?",
        &[RuntimeLocalImage {
            path: path.to_string_lossy().into_owned(),
        }],
    )
    .unwrap();
    assert_eq!(message["type"], "user");
    let content = message["message"]["content"].as_array().unwrap();
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "what color?");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["source"]["type"], "base64");
    assert_eq!(content[1]["source"]["media_type"], "image/png");
    assert!(content[1]["source"]["data"].as_str().unwrap().len() > 0);
}
