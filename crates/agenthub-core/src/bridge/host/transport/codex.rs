use axum::response::Response;
use reqwest::RequestBuilder;
use serde_json::Value;

use crate::bridge::grok_cli::GrokCliRequestIdentity;
use crate::bridge::protocol::responses::{prepare_official_codex_request, to_responses_request};

use super::super::admission::AdmittedRequest;
use super::super::surface::DownstreamSurface;
use super::{
    models_surface_unreachable, parse_bridge_request, passthrough_responses_object, RecoveryPolicy,
    UpstreamDecode, UpstreamPrepare, UpstreamTransport,
};

pub(super) struct CodexTransport;

impl UpstreamTransport for CodexTransport {
    fn passthrough(&self) -> bool {
        true
    }

    fn path(&self) -> &'static str {
        "responses"
    }

    fn apply_auth(
        &self,
        builder: RequestBuilder,
        token: &str,
        _grok_identity: Option<&GrokCliRequestIdentity>,
    ) -> RequestBuilder {
        builder.bearer_auth(token)
    }

    fn prepare(
        &self,
        surface: DownstreamSurface,
        admitted: &AdmittedRequest,
    ) -> Result<UpstreamPrepare, Response> {
        match surface {
            DownstreamSurface::Responses => {
                let incoming_model = admitted
                    .body
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let (mut body, stream) = passthrough_responses_object(admitted.body.clone())?;
                prepare_official_codex_request(
                    &mut body,
                    &incoming_model,
                    admitted.state.upstream.model.as_deref(),
                );
                Ok(UpstreamPrepare {
                    path: self.path(),
                    body,
                    grok_identity: None,
                    cache_seed: None,
                    stream,
                })
            }
            DownstreamSurface::Messages | DownstreamSurface::ChatCompletions => {
                let request = parse_bridge_request(surface, admitted)?;
                let stream = request.stream;
                let mut body = to_responses_request(&request);
                prepare_official_codex_request(
                    &mut body,
                    &request.model,
                    admitted.state.upstream.model.as_deref(),
                );
                Ok(UpstreamPrepare {
                    path: self.path(),
                    body,
                    grok_identity: None,
                    cache_seed: None,
                    stream,
                })
            }
            DownstreamSurface::Models => models_surface_unreachable(),
        }
    }

    fn decode_kind(&self) -> UpstreamDecode {
        UpstreamDecode::OpenAiResponses
    }

    fn recovery(&self) -> RecoveryPolicy {
        RecoveryPolicy::Oauth401Reload
    }
}
