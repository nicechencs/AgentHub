use super::*;
use serde_json::json;

#[test]
fn expand_binary_names_adds_windows_suffixes() {
    let names = expand_binary_names("codex");
    assert!(names.iter().any(|n| n == "codex"));
    #[cfg(windows)]
    {
        assert!(names.iter().any(|n| n == "codex.cmd"));
        assert!(names.iter().any(|n| n == "codex.exe"));
    }
}

#[test]
fn well_known_paths_include_codex_npm_and_native_homes() {
    let paths = well_known_bin_paths(AgentId::Codex);
    assert!(!paths.is_empty(), "Codex must have well-known fallbacks");
    assert!(
        paths.iter().any(|(_, ch)| *ch == "npm"),
        "Codex should probe npm global bin"
    );
    let kimi = well_known_bin_paths(AgentId::Kimi);
    assert!(kimi
        .iter()
        .any(|(p, _)| p.to_string_lossy().contains("kimi-code")));
    let grok = well_known_bin_paths(AgentId::Grok);
    assert!(grok
        .iter()
        .any(|(p, _)| p.to_string_lossy().contains(".grok")));
}

#[test]
fn well_known_paths_cover_all_agents_non_empty() {
    for agent in AgentId::ALL {
        let paths = well_known_bin_paths(agent);
        assert!(
            !paths.is_empty(),
            "{} must have at least one well-known path",
            agent.as_str()
        );
        for (p, ch) in &paths {
            assert!(
                *ch == "npm" || *ch == "native",
                "channel must be npm|native, got {ch} for {}",
                p.display()
            );
        }
    }
}

#[test]
fn infer_channel_from_npm_and_native_paths() {
    #[cfg(windows)]
    {
        let npm = PathBuf::from(r"C:\Users\demo\AppData\Roaming\npm\codex.cmd");
        assert_eq!(infer_channel(&npm, None), "npm");
        let native = PathBuf::from(r"C:\Users\demo\.grok\bin\grok.exe");
        assert_eq!(infer_channel(&native, None), "native");
        let kimi = PathBuf::from(r"C:\Users\demo\.kimi-code\bin\kimi.exe");
        assert_eq!(infer_channel(&kimi, Some("native")), "native");
    }
    #[cfg(not(windows))]
    {
        let native = PathBuf::from("/Users/demo/.grok/bin/grok");
        assert_eq!(infer_channel(&native, None), "native");
        let npm_global = PathBuf::from("/Users/demo/.npm-global/bin/codex");
        assert_eq!(infer_channel(&npm_global, None), "npm");
    }
}

#[test]
fn looks_like_version_line_accepts_cli_versions() {
    assert!(looks_like_version_line("2.1.220 (Claude Code)"));
    assert!(looks_like_version_line("codex-cli 0.144.5"));
    assert!(looks_like_version_line("0.29.1"));
    assert!(looks_like_version_line("grok 0.2.118 (1e1687c1cf)"));
    assert!(looks_like_version_line("claude"));
    // pi --version prints bare semver (measured: "0.83.0")
    assert!(looks_like_version_line("0.83.0"));
    assert!(looks_like_version_line("pi 0.83.0"));
}

#[test]
fn extract_version_token_strips_cli_name_noise() {
    assert_eq!(extract_version_token("codex-cli 0.144.5"), "0.144.5");
    assert_eq!(extract_version_token("2.1.220 (Claude Code)"), "2.1.220");
    assert_eq!(extract_version_token("grok 0.2.118 (1e1687c1cf)"), "0.2.118");
    assert_eq!(extract_version_token("0.83.0"), "0.83.0");
    assert_eq!(extract_version_token("v1.2.3"), "1.2.3");
    assert_eq!(extract_version_token("pi 0.83.0"), "0.83.0");
}

#[test]
fn looks_like_version_line_rejects_shell_errors() {
    assert!(!looks_like_version_line(""));
    assert!(!looks_like_version_line(
        "'node' is not recognized as an internal or external command"
    ));
    assert!(!looks_like_version_line("command not found: node"));
    assert!(!looks_like_version_line("不是内部或外部命令"));
    assert!(!looks_like_version_line("系统找不到指定的文件"));
    // too long
    assert!(!looks_like_version_line(&"x".repeat(200)));
}

#[test]
fn not_found_firefighting_note_mentions_path_and_restart() {
    assert!(NOT_FOUND_FIREFIGHTING_NOTE.contains("PATH"));
    assert!(NOT_FOUND_FIREFIGHTING_NOTE.contains("well-known"));
    assert!(
        NOT_FOUND_FIREFIGHTING_NOTE.contains("restart")
            || NOT_FOUND_FIREFIGHTING_NOTE.contains("re-detect")
    );
}

#[test]
fn detect_binary_prefers_path_or_well_known_when_agent_installed() {
    // Integration smoke on developer machines: at least one of claude/codex/kimi/grok
    // is commonly present. If none, still validates NotFound note shape.
    let mut any = false;
    for (agent, name) in [
        (AgentId::Claude, "claude"),
        (AgentId::Codex, "codex"),
        (AgentId::Kimi, "kimi"),
        (AgentId::Grok, "grok"),
        (AgentId::Pi, "pi"),
    ] {
        let r = detect_binary(agent, &[name], &["--version"], None, true);
        if r.status == crate::models::DetectStatus::Installed {
            any = true;
            assert!(r.binary_path.is_some());
            assert!(
                r.channel.as_deref() == Some("npm") || r.channel.as_deref() == Some("native"),
                "channel must be concrete npm|native, got {:?}",
                r.channel
            );
            // Version may be empty if probe fails; must never be shell-error garbage.
            if let Some(v) = &r.version {
                assert!(
                    looks_like_version_line(v),
                    "version must look like a version, got {v:?}"
                );
            }
        } else {
            assert_eq!(r.notes, vec![NOT_FOUND_FIREFIGHTING_NOTE.to_string()]);
        }
    }
    let _ = any;
}

#[test]
fn json_writer_replaces_complete_claude_config() {
    let dir = tempfile::tempdir().unwrap();
    let json_path = dir.path().join("settings.json");
    std::fs::write(&json_path, b"old").unwrap();
    let claude = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "secret"}}),
    };
    write_json_config(&json_path, &claude).unwrap();
    let stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&json_path).unwrap()).unwrap();
    assert_eq!(stored, claude.raw);
}

#[test]
fn toml_writer_replaces_provider_keys_and_preserves_unmanaged_sections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let live = r#"# user comment
model = "old-model"
base_url = "https://old.example"
api_key = "old-secret"
theme = "dark"

[mcp_servers.demo]
command = "demo"
"#;
    std::fs::write(&path, live).unwrap();
    let desired = r#"model = "new-model"
api_key = "new-secret"
"#;
    let grok = AgentConfig {
        agent: AgentId::Grok,
        raw: json!({"format": "toml", "content": desired}),
    };

    write_toml_config(AgentId::Grok, &path, &grok).unwrap();

    let stored = std::fs::read_to_string(&path).unwrap();
    assert!(stored.contains("# user comment"));
    assert!(stored.contains("theme = \"dark\""));
    assert!(stored.contains("[mcp_servers.demo]"));
    assert!(stored.contains("command = \"demo\""));
    assert!(stored.contains("model = \"new-model\""));
    assert!(stored.contains("api_key = \"new-secret\""));
    assert!(!stored.contains("old-model"));
    assert!(!stored.contains("old.example"));
    assert!(!stored.contains("old-secret"));
    assert!(!stored.contains("base_url"));
}

#[test]
fn toml_writer_accepts_ccswitch_config_alias() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let codex = AgentConfig {
        agent: AgentId::Codex,
        raw: json!({
            "format": "toml",
            "config": "model = \"from-config-alias\"\n",
            "auth": { "OPENAI_API_KEY": "sk-x" }
        }),
    };
    write_toml_config(AgentId::Codex, &path, &codex).unwrap();
    let stored = std::fs::read_to_string(&path).unwrap();
    assert!(stored.contains("from-config-alias"));
}

#[test]
fn toml_writer_replaces_codex_provider_table_but_keeps_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let live = r#"model = "old"

[model_providers.old]
base_url = "https://old.example"

# keep this integration
[mcp_servers.keep]
command = "keep"
"#;
    std::fs::write(&path, live).unwrap();
    let desired = r#"model = "new"

[model_providers.new]
base_url = "https://new.example"
"#;
    let codex = AgentConfig {
        agent: AgentId::Codex,
        raw: json!({"format": "toml", "content": desired}),
    };

    write_toml_config(AgentId::Codex, &path, &codex).unwrap();

    let stored = std::fs::read_to_string(&path).unwrap();
    assert!(stored.contains("model = \"new\""));
    assert!(stored.contains("[model_providers.new]"));
    assert!(stored.contains("https://new.example"));
    assert!(!stored.contains("[model_providers.old]"));
    assert!(!stored.contains("https://old.example"));
    assert!(stored.contains("# keep this integration"));
    assert!(stored.contains("[mcp_servers.keep]"));
}

#[test]
fn toml_writer_rejects_invalid_documents_without_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let valid_live = "model = \"old\"\n";
    std::fs::write(&path, valid_live).unwrap();

    let invalid_target = AgentConfig {
        agent: AgentId::Grok,
        raw: json!({"format": "toml", "content": "model = ["}),
    };
    assert_eq!(
        write_toml_config(AgentId::Grok, &path, &invalid_target)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), valid_live);

    let invalid_live = "not valid toml";
    std::fs::write(&path, invalid_live).unwrap();
    let valid_target = AgentConfig {
        agent: AgentId::Grok,
        raw: json!({"format": "toml", "content": "model = \"new\"\n"}),
    };
    assert_eq!(
        write_toml_config(AgentId::Grok, &path, &valid_target)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid_live);
}

#[test]
fn config_writers_reject_agent_and_shape_mismatches_without_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, b"keep").unwrap();

    let wrong_agent = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"format": "toml", "content": "replace"}),
    };
    assert_eq!(
        write_toml_config(AgentId::Grok, &path, &wrong_agent)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );

    let wrong_shape = AgentConfig {
        agent: AgentId::Grok,
        raw: json!({"format": "toml", "preview": "truncated"}),
    };
    assert_eq!(
        write_toml_config(AgentId::Grok, &path, &wrong_shape)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
}

#[test]
fn matrix_covers_all_agents_and_capabilities() {
    let reg = register_all();
    let matrix = reg.matrix();
    assert_eq!(matrix.len(), AgentId::ALL.len());
    for agent in AgentId::ALL {
        let row = matrix.get(&agent).expect("agent row");
        assert_eq!(row.len(), Capability::ALL.len());
    }
}

#[test]
fn non_full_capabilities_must_explain_themselves() {
    for adapter in register_all().all() {
        for cap in Capability::ALL {
            let state = adapter.capability(cap);
            if state.level != CapabilityLevel::Full {
                assert!(
                    state.reason.is_some(),
                    "{}/{:?} 缺少 reason",
                    adapter.id().as_str(),
                    cap
                );
            }
        }
    }
}

#[test]
fn declared_capabilities_match_actual_behavior() {
    for adapter in register_all().all() {
        let id = adapter.id();

        if adapter.capability(Capability::Skills).is_blocked() {
            assert!(
                adapter.skills_dir().is_none(),
                "{} 声明无 skills 却给了目录",
                id.as_str()
            );
        }
        if adapter.capability(Capability::ConfigWrite).is_blocked() {
            let probe = AgentConfig {
                agent: id,
                raw: json!({}),
            };
            assert!(
                matches!(adapter.write_config(&probe), Err(AppError::Unsupported(_))),
                "{} 声明无 ConfigWrite 但 write_config 未 fail-closed",
                id.as_str()
            );
        }
        if adapter.capability(Capability::LiveBackup).is_blocked() {
            assert!(
                adapter.live_backup_paths().is_empty(),
                "{} 声明无 LiveBackup 却返回路径",
                id.as_str()
            );
        }
        if adapter
            .capability(Capability::StructuredStream)
            .is_blocked()
        {
            assert!(
                !supports_structured_stream(id),
                "{} 声明无 StructuredStream",
                id.as_str()
            );
        }
        if adapter.capability(Capability::ProviderPresets).is_blocked() {
            assert!(
                crate::presets::list_for(id).is_empty(),
                "{} 声明无 ProviderPresets 却有预设",
                id.as_str()
            );
        }
    }
}

#[test]
fn require_blocks_unsupported_and_allows_full() {
    let reg = register_all();
    assert!(reg.require(AgentId::Claude, Capability::Skills).is_ok());
    let err = match reg.require(AgentId::Kimi, Capability::Skills) {
        Ok(_) => panic!("kimi skills should be blocked"),
        Err(e) => e,
    };
    assert_eq!(err.code(), "unsupported");
    assert!(err.to_string().contains("技能"));
    assert!(err.to_string().contains("不支持"));
}

#[test]
fn require_planned_uses_distinct_copy_from_unsupported() {
    let reg = register_all();
    // Claude Usage is Full (parser wired); SessionResume still Planned; Kimi Skills Unsupported.
    assert!(reg.require(AgentId::Claude, Capability::Usage).is_ok());
    let planned = match reg.require(AgentId::Claude, Capability::SessionResume) {
        Ok(_) => panic!("claude session resume should be planned/blocked"),
        Err(e) => e,
    };
    let unsupported = match reg.require(AgentId::Kimi, Capability::Skills) {
        Ok(_) => panic!("kimi skills should be unsupported"),
        Err(e) => e,
    };
    assert_eq!(planned.code(), "unsupported");
    assert_eq!(unsupported.code(), "unsupported");
    assert!(
        planned.to_string().contains("尚未接入"),
        "planned copy: {}",
        planned
    );
    assert!(
        unsupported.to_string().contains("不支持"),
        "unsupported copy: {}",
        unsupported
    );
    assert!(!planned.to_string().contains("不支持"));
}

#[test]
fn require_allows_partial_dangerous_mode_for_kimi() {
    let reg = register_all();
    let adapter = reg
        .require(AgentId::Kimi, Capability::DangerousMode)
        .expect("partial should pass");
    assert_eq!(adapter.id(), AgentId::Kimi);
    let state = adapter.capability(Capability::DangerousMode);
    assert_eq!(state.level, CapabilityLevel::Partial);
    assert!(state.reason.is_some());
}

#[test]
fn matrix_matches_documented_boundary_cells() {
    let reg = register_all();
    let matrix = reg.matrix();
    assert_eq!(
        matrix[&AgentId::Kimi][&Capability::Skills].level,
        CapabilityLevel::Unsupported
    );
    assert_eq!(
        matrix[&AgentId::Cursor][&Capability::AccountSwitch].level,
        CapabilityLevel::Unsupported
    );
    assert_eq!(
        matrix[&AgentId::Codex][&Capability::ApiKeyAccount].level,
        CapabilityLevel::Partial
    );
    assert_eq!(
        matrix[&AgentId::Cursor][&Capability::Usage].level,
        CapabilityLevel::Unsupported
    );
    assert_eq!(
        matrix[&AgentId::Claude][&Capability::Usage].level,
        CapabilityLevel::Full
    );
    assert!(supports_structured_stream(AgentId::Claude));
    assert!(!supports_structured_stream(AgentId::WorkBuddy));
    assert!(!supports_structured_stream(AgentId::Cursor));
}

#[test]
fn write_verified_json_object_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("creds.json");
    let body = serde_json::json!({"format": "api_key", "api_key": "sk-test"});
    write_verified_json_object(&path, &body).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(parsed, body);
}

#[test]
fn write_verified_json_object_rejects_non_object() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.json");
    let err = write_verified_json_object(&path, &serde_json::json!(["x"])).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn require_api_key_and_live_account_helpers() {
    assert_eq!(require_api_key("  sk-abc  ").unwrap(), "sk-abc");
    assert_eq!(require_api_key("").unwrap_err().code(), "invalid_arg");
    let live = api_key_live_account(
        AgentId::Claude,
        "sk-abcdefghijklmnop",
        serde_json::json!({"format": "api_key", "api_key": "sk-abcdefghijklmnop"}),
        "API Key",
        serde_json::json!({"source": "manual"}),
    );
    assert_eq!(live.agent, AgentId::Claude);
    assert_eq!(live.kind, AccountKind::ApiKey);
    assert!(live.label_hint.as_ref().unwrap().contains("API Key"));
    assert!(!live
        .label_hint
        .as_ref()
        .unwrap()
        .contains("sk-abcdefghijklmnop"));
}

#[test]
fn supports_structured_stream_uses_shared_registry() {
    // Twice: must not rebuild via register_all each call (OnceLock).
    assert_eq!(
        supports_structured_stream(AgentId::Claude),
        supports_structured_stream(AgentId::Claude)
    );
    assert!(supports_structured_stream(AgentId::Grok));
    assert!(!supports_structured_stream(AgentId::Cursor));
}


#[test]
fn default_authorization_key_api_key_stable_and_distinct() {
    let a = json!({"format": "api_key", "api_key": "sk-same"});
    let b = json!({"format": "api_key", "api_key": "sk-same"});
    let c = json!({"format": "api_key", "api_key": "sk-other"});
    let ka = default_authorization_key(AccountKind::ApiKey, &a).unwrap();
    let kb = default_authorization_key(AccountKind::ApiKey, &b).unwrap();
    let kc = default_authorization_key(AccountKind::ApiKey, &c).unwrap();
    assert_eq!(ka, kb);
    assert_ne!(ka, kc);
    assert!(ka.starts_with("apikey:sha256:"));
    // Must not embed raw key
    assert!(!ka.contains("sk-same"));
}

#[test]
fn default_authorization_key_oauth_prefers_refresh_then_access() {
    let with_refresh = json!({
        "format": "credentials_json",
        "body": {
            "refresh_token": "refresh-aaa",
            "access_token": "access-should-be-ignored"
        }
    });
    let with_access = json!({
        "format": "auth_json",
        "body": { "provider": { "email": "u@x.com", "key": "access-bbb" } }
    });
    let kr = default_authorization_key(AccountKind::Oauth, &with_refresh).unwrap();
    let ka = default_authorization_key(AccountKind::Oauth, &with_access).unwrap();
    assert!(kr.starts_with("oauth:refresh_sha:"));
    assert!(ka.starts_with("oauth:access_sha:"));
    assert_ne!(kr, ka);

    // Same email, different access keys → different authorizations
    let other = json!({
        "format": "auth_json",
        "body": { "provider": { "email": "u@x.com", "key": "access-ccc" } }
    });
    assert_ne!(
        default_authorization_key(AccountKind::Oauth, &with_access).unwrap(),
        default_authorization_key(AccountKind::Oauth, &other).unwrap()
    );
}

#[test]
fn default_identity_label_uses_email_not_for_auth_key() {
    let creds = json!({
        "format": "auth_json",
        "body": { "provider": { "email": "person@example.com", "key": "tok" } }
    });
    let label = default_identity_label(AccountKind::Oauth, &creds, Some("hint")).unwrap();
    assert_eq!(label, "person@example.com");
    let key = default_authorization_key(AccountKind::Oauth, &creds).unwrap();
    assert!(!key.contains("person@example.com"));
    assert!(key.starts_with("oauth:access_sha:"));
}
