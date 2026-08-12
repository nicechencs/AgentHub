use super::*;

#[test]
fn command_input_only_accepts_connection_table_kinds() {
    assert_eq!(
        parse_source_kind("account").unwrap(),
        AdapterSourceKind::Account
    );
    assert_eq!(
        parse_source_kind("provider").unwrap(),
        AdapterSourceKind::Provider
    );
    assert!(parse_source_kind("credential").is_err());
}

#[test]
fn optional_source_kind_preserves_absent_list_filter() {
    assert_eq!(parse_source_kind_opt(None).unwrap(), None);
    assert_eq!(
        parse_source_kind_opt(Some("provider")).unwrap(),
        Some(AdapterSourceKind::Provider)
    );
    assert!(parse_source_kind_opt(Some("credential")).is_err());
}
