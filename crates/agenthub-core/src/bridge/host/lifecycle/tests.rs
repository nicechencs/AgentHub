use super::same_spec;
use crate::bridge::runtime::{
    BridgeLocalSurface, BridgeStartSpec, BridgeUpstreamConfig, BridgeUpstreamProtocol,
    DownstreamResponsesProfile, ResolvedAuth, ResponsesDialect,
};

fn spec_with_profile(profile: Option<DownstreamResponsesProfile>) -> BridgeStartSpec {
    BridgeStartSpec::new(
        "profile",
        0,
        "local-token",
        BridgeUpstreamConfig {
            base_url: "http://127.0.0.1/v1/".to_owned(),
            model: Some("model".to_owned()),
            source_id: None,
            auth: ResolvedAuth::bearer("upstream-token"),
            protocol: BridgeUpstreamProtocol::XaiResponsesOauth,
            local_surface: BridgeLocalSurface::Responses,
        },
    )
    .with_downstream_responses_profile(profile)
}

#[test]
fn same_spec_detects_downstream_responses_profile_change() {
    let generic = spec_with_profile(None);
    let codex = spec_with_profile(Some(DownstreamResponsesProfile::new(
        ResponsesDialect::Codex,
    )));
    let grok = spec_with_profile(Some(DownstreamResponsesProfile::new(
        ResponsesDialect::Grok,
    )));

    assert!(same_spec(&generic, &generic));
    assert!(!same_spec(&generic, &codex));
    assert!(!same_spec(&codex, &grok));
}
