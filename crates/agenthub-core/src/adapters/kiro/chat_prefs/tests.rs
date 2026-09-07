use super::*;

#[test]
fn parse_list_models_json_reads_ids_and_default() {
    let stdout = r#"{
          "models": [
            {"model_name":"auto","model_id":"auto"},
            {"model_name":"claude-haiku-4.5","model_id":"claude-haiku-4.5"}
          ],
          "default_model":"auto"
        }"#;
    let (default, models) = parse_kiro_list_models_json(stdout);
    assert_eq!(default.as_deref(), Some("auto"));
    assert_eq!(
        models,
        vec!["auto".to_string(), "claude-haiku-4.5".to_string()]
    );
}

#[test]
fn parse_list_models_json_inserts_missing_default() {
    let stdout = r#"{"models":[{"model_id":"a"}],"default_model":"b"}"#;
    let (default, models) = parse_kiro_list_models_json(stdout);
    assert_eq!(default.as_deref(), Some("b"));
    assert_eq!(models, vec!["b".to_string(), "a".to_string()]);
}

#[test]
fn parse_list_models_json_rejects_garbage() {
    assert_eq!(parse_kiro_list_models_json("not-json"), (None, Vec::new()));
}

#[test]
fn normalize_effort_accepts_documented_values() {
    assert_eq!(normalize_effort("medium").as_deref(), Some("medium"));
    assert_eq!(normalize_effort(" max ").as_deref(), Some("max"));
    assert_eq!(normalize_effort("turbo"), None);
}
