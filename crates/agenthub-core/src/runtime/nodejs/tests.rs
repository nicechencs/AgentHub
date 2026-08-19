use super::*;
use crate::catalog::limits::{NODE_MIN_MAJOR, PI_NODE_MIN_MAJOR};
use std::path::Path;

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

fn touch_node(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(if cfg!(windows) { "node.exe" } else { "node" });
    std::fs::write(&path, b"").unwrap();
    path
}

#[test]
fn node_home_candidates_include_local_share_nvm_fnm_volta_n() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let share22 = touch_node(
        &home
            .join(".local")
            .join("share")
            .join("node-v22.19.0")
            .join("bin"),
    );
    let _share20 = touch_node(
        &home
            .join(".local")
            .join("share")
            .join("node-v20.11.0")
            .join("bin"),
    );
    let nvm22 = touch_node(
        &home
            .join(".nvm")
            .join("versions")
            .join("node")
            .join("v22.20.0")
            .join("bin"),
    );
    let _nvm18 = touch_node(
        &home
            .join(".nvm")
            .join("versions")
            .join("node")
            .join("v18.20.0")
            .join("bin"),
    );
    let fnm22 = touch_node(
        &home
            .join(".fnm")
            .join("node-versions")
            .join("v22.19.0")
            .join("installation")
            .join("bin"),
    );
    let volta = touch_node(&home.join(".volta").join("bin"));
    let n_bin = touch_node(&home.join("n").join("bin"));
    let n_ver = touch_node(
        &home
            .join("n")
            .join("versions")
            .join("node")
            .join("22.19.0")
            .join("bin"),
    );

    let cands = node_versioned_home_candidates(home, 22);
    assert!(cands.contains(&share22), "missing local share: {cands:?}");
    assert!(cands.contains(&nvm22), "missing nvm: {cands:?}");
    assert!(cands.contains(&fnm22), "missing fnm: {cands:?}");
    assert!(cands.contains(&volta), "missing volta: {cands:?}");
    assert!(cands.contains(&n_bin), "missing n prefix: {cands:?}");
    assert!(cands.contains(&n_ver), "missing n version: {cands:?}");
    assert!(
        !cands
            .iter()
            .any(|p| p.to_string_lossy().contains("node-v20")),
        "Node 20 home tree must not be a >=22 candidate: {cands:?}"
    );
    assert!(
        !cands
            .iter()
            .any(|p| p.to_string_lossy().contains("v18.20.0")),
        "Node 18 nvm tree must not be a >=22 candidate: {cands:?}"
    );
}

#[test]
fn resolve_skips_path_node20_for_home_node22() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let home22 = touch_node(
        &home
            .join(".local")
            .join("share")
            .join("node-v22.19.0")
            .join("bin"),
    );
    let path20 = PathBuf::from("/usr/bin/node");
    let picked = resolve_node_at_least_from(
        Some(path20.clone()),
        Some(home),
        22,
        NodeManagerRoots::from_home_only(home),
        |p| {
            if p == &path20 {
                Some(("20.18.0".into(), 20))
            } else if p == &home22 {
                Some(("22.19.0".into(), 22))
            } else {
                None
            }
        },
    )
    .expect("home Node 22");
    assert_eq!(picked.path, home22);
    assert_eq!(picked.major, 22);
    assert_eq!(picked.version, "22.19.0");
    assert_eq!(picked.bin_dir().as_deref(), home22.parent());
}

#[test]
fn resolve_returns_none_when_only_node20_on_path() {
    let path20 = PathBuf::from("/usr/bin/node");
    let picked = resolve_node_at_least_from(
        Some(path20.clone()),
        None,
        22,
        NodeManagerRoots::default(),
        |p| {
            if p == &path20 {
                Some(("20.18.0".into(), 20))
            } else {
                None
            }
        },
    );
    assert!(
        picked.is_none(),
        "PATH Node 20 must not satisfy Pi Node 22: {picked:?}"
    );
}

#[test]
fn resolve_prefers_path_node22() {
    let path22 = PathBuf::from("/opt/node22/bin/node");
    let picked = resolve_node_at_least_from(
        Some(path22.clone()),
        None,
        22,
        NodeManagerRoots::default(),
        |p| {
            if p == &path22 {
                Some(("22.19.0".into(), 22))
            } else {
                Some(("20.0.0".into(), 20))
            }
        },
    )
    .expect("Node 22 on PATH");
    assert_eq!(picked.path, path22);
    assert_eq!(picked.major, 22);
    assert_eq!(
        picked.bin_dir().as_deref(),
        Some(Path::new("/opt/node22/bin"))
    );
}

#[test]
fn path_prefix_puts_node22_bin_first() {
    let prefixed = path_with_prefixed_bin(Path::new("/n22/bin"), "/usr/bin:/bin");
    #[cfg(windows)]
    assert_eq!(prefixed, r"/n22/bin;/usr/bin:/bin");
    #[cfg(not(windows))]
    assert_eq!(prefixed, "/n22/bin:/usr/bin:/bin");
    assert_eq!(
        path_with_prefixed_bin(Path::new("/n22/bin"), ""),
        "/n22/bin"
    );
}

#[test]
fn global_node_min_major_stays_18() {
    assert_eq!(NODE_MIN_MAJOR, 18);
    assert_eq!(PI_NODE_MIN_MAJOR, 22);
}
