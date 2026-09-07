use super::*;

#[test]
fn host_runtimes_skip_powershell_outside_windows() {
    let ids: Vec<_> = host_runtimes().iter().copied().collect();
    #[cfg(windows)]
    {
        assert!(ids.contains(&RuntimeId::PowerShell));
        assert_eq!(ids.len(), RuntimeId::ALL.len());
    }
    #[cfg(not(windows))]
    {
        assert!(!ids.contains(&RuntimeId::PowerShell));
        assert_eq!(ids, vec![RuntimeId::NodeJs, RuntimeId::Npm, RuntimeId::Git]);
    }
}

#[test]
fn detect_all_matches_host_runtimes() {
    let statuses = detect_all();
    assert_eq!(statuses.len(), host_runtimes().len());
    assert!(statuses
        .iter()
        .all(|s| s.id != RuntimeId::PowerShell || cfg!(windows)));
}
