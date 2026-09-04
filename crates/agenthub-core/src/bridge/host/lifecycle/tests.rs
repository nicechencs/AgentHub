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

#[tokio::test]
async fn bind_loopback_ephemeral_port() {
    let listener = super::bind_loopback(0).expect("bind");
    let addr = listener.local_addr().expect("addr");
    assert_eq!(
        addr.ip(),
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
    );
    assert_ne!(addr.port(), 0);
}

#[cfg(windows)]
#[tokio::test]
async fn windows_bind_loopback_is_exclusive() {
    let first = super::bind_loopback(0).expect("first bind");
    let port = first.local_addr().expect("addr").port();
    let second = super::bind_loopback(port);
    assert!(
        second.is_err(),
        "second bind to the same loopback port must fail under SO_EXCLUSIVEADDRUSE"
    );
    drop(first);
}
