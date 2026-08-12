use super::*;

#[test]
fn parse_major_ok() {
    assert_eq!(parse_major("22.11.0"), Some(22));
    assert_eq!(parse_major("7.6.4"), Some(7));
}

#[test]
fn parse_git_version_strips_prefix() {
    assert_eq!(
        parse_git_version("git version 2.43.0.windows.1"),
        "2.43.0.windows.1"
    );
    assert_eq!(parse_git_version("git version 2.39.2"), "2.39.2");
    assert_eq!(parse_git_version("2.40.0"), "2.40.0");
}

#[test]
fn detect_git_returns_git_runtime_id() {
    let st = detect_git();
    assert_eq!(st.id, RuntimeId::Git);
    match st.status {
        EnvStatusKind::Ok => {
            assert!(st.path.is_some());
            assert!(st.version.is_some());
        }
        EnvStatusKind::Missing => assert!(st.path.is_none()),
        EnvStatusKind::BrokenPath => assert!(st.path.is_some()),
        EnvStatusKind::Outdated => panic!("git has no min version yet"),
    }
}

#[test]
fn detect_powershell_emits_dual_version_notes() {
    let st = detect_powershell();
    assert!(!st.notes.is_empty(), "PowerShell detect must emit notes");
    assert_eq!(st.id, RuntimeId::PowerShell);
    #[cfg(windows)]
    {
        assert!(
            st.notes
                .iter()
                .any(|n| n.contains("Windows PowerShell 5.1")),
            "Windows must report 5.1 line: {:?}",
            st.notes
        );
        assert!(
            st.notes.iter().any(|n| n.contains("PowerShell 7")),
            "Windows must report pwsh line: {:?}",
            st.notes
        );
    }
    #[cfg(not(windows))]
    {
        assert_eq!(st.status, EnvStatusKind::Ok);
        assert!(st.path.is_none());
        assert!(
            st.notes
                .iter()
                .any(|n| n.contains("not applicable") || n.contains("not required")),
            "non-Windows must mark PowerShell as not applicable/required: {:?}",
            st.notes
        );
        assert!(
            !st.notes
                .iter()
                .any(|n| n.contains("Windows PowerShell 5.1:") && n.contains('@')),
            "non-Windows must not report a 5.1 binary path: {:?}",
            st.notes
        );
    }
}

#[test]
fn resolve_powershell_for_native_prefers_existing_binary() {
    let resolved = resolve_powershell_for_native();
    #[cfg(windows)]
    {
        assert!(
            resolved.is_some(),
            "Windows should resolve at least System32 powershell or pwsh"
        );
        let p = resolved.unwrap();
        assert!(
            p.is_file(),
            "resolved PowerShell must exist: {}",
            p.display()
        );
    }
    #[cfg(not(windows))]
    {
        if let Some(p) = resolved {
            assert!(p.is_file());
        }
    }
}

#[test]
fn homebrew_binary_candidates_are_absolute_and_cover_npm() {
    let candidates = homebrew_binary_candidates(&["node", "npm"]);
    assert!(candidates.contains(&PathBuf::from("/opt/homebrew/bin/node")));
    assert!(candidates.contains(&PathBuf::from("/opt/homebrew/bin/npm")));
    assert!(candidates.contains(&PathBuf::from("/usr/local/bin/node")));
    assert!(candidates.contains(&PathBuf::from("/usr/local/bin/npm")));
}

#[test]
fn platform_binary_candidates_only_enable_homebrew_on_macos() {
    let candidates = platform_binary_candidates(&["node", "npm"]);
    #[cfg(target_os = "macos")]
    assert_eq!(candidates, homebrew_binary_candidates(&["node", "npm"]));
    #[cfg(not(target_os = "macos"))]
    assert!(candidates.is_empty());
}
