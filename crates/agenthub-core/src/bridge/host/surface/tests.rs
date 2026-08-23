use crate::bridge::runtime::BridgeLocalSurface;

use super::DownstreamSurface;

#[test]
fn from_local_matches_conversation_surfaces() {
    assert_eq!(
        DownstreamSurface::from_local(BridgeLocalSurface::Responses),
        DownstreamSurface::Responses
    );
    assert_eq!(
        DownstreamSurface::from_local(BridgeLocalSurface::Messages),
        DownstreamSurface::Messages
    );
    assert_eq!(
        DownstreamSurface::from_local(BridgeLocalSurface::ChatCompletions),
        DownstreamSurface::ChatCompletions
    );
}

#[test]
fn models_always_served_conversation_surfaces_must_match() {
    for local in [
        BridgeLocalSurface::Responses,
        BridgeLocalSurface::Messages,
        BridgeLocalSurface::ChatCompletions,
    ] {
        assert!(DownstreamSurface::Models.served_by(local));
        assert!(DownstreamSurface::from_local(local).served_by(local));
    }
    assert!(!DownstreamSurface::Responses.served_by(BridgeLocalSurface::Messages));
    assert!(!DownstreamSurface::Responses.served_by(BridgeLocalSurface::ChatCompletions));
    assert!(!DownstreamSurface::Messages.served_by(BridgeLocalSurface::Responses));
    assert!(!DownstreamSurface::Messages.served_by(BridgeLocalSurface::ChatCompletions));
    assert!(!DownstreamSurface::ChatCompletions.served_by(BridgeLocalSurface::Responses));
    assert!(!DownstreamSurface::ChatCompletions.served_by(BridgeLocalSurface::Messages));
}

#[test]
fn op_strings_match_existing_log_contract() {
    assert_eq!(DownstreamSurface::Responses.op(), "responses");
    assert_eq!(DownstreamSurface::Messages.op(), "messages");
    assert_eq!(DownstreamSurface::ChatCompletions.op(), "chat");
    assert_eq!(DownstreamSurface::Models.op(), "models");
}
