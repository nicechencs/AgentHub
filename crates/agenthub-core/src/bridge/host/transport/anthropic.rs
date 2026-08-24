use axum::response::Response;
use reqwest::RequestBuilder;

use crate::bridge::grok_cli::GrokCliRequestIdentity;
use crate::bridge::protocol::anthropic_messages::to_anthropic_messages_request;

use super::super::admission::AdmittedRequest;
use super::super::surface::DownstreamSurface;
use super::super::ANTHROPIC_API_VERSION;
use super::{
    models_surface_unreachable, overwrite_configured_model, overwrite_configured_model_with,
    parse_bridge_request, passthrough_responses_object, RecoveryPolicy, UpstreamDecode,
    UpstreamPrepare, UpstreamTransport,
};

pub(super) struct AnthropicTransport;

impl UpstreamTransport for AnthropicTransport {
    fn path(&self) -> &'static str {
        "messages"
    }

    fn apply_auth(
        &self,
        builder: RequestBuilder,
        token: &str,
        _grok_identity: Option<&GrokCliRequestIdentity>,
    ) -> RequestBuilder {
        builder
            .header("x-api-key", token)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
    }

    fn prepare(
        &self,
        surface: DownstreamSurface,
        admitted: &AdmittedRequest,
    ) -> Result<UpstreamPrepare, Response> {
        match surface {
            DownstreamSurface::Responses => {
                let request = parse_bridge_request(surface, admitted)?;
                let stream = request.stream;
                let mut body = to_anthropic_messages_request(&request);
                overwrite_configured_model(
                    &mut body,
                    admitted.state.upstream.model.as_deref(),
                    &admitted.state.listed_models,
                );
                Ok(UpstreamPrepare {
                    path: self.path(),
                    body,
                    grok_identity: None,
                    cache_seed: None,
                    stream,
                })
            }
            DownstreamSurface::Messages => {
                let (mut body, stream) = passthrough_responses_object(admitted.body.clone())?;
                overwrite_configured_model_with(
                    &mut body,
                    admitted.state.upstream.model.as_deref(),
                    true,
                    &admitted.state.listed_models,
                );
                Ok(UpstreamPrepare {
                    path: self.path(),
                    body,
                    grok_identity: None,
                    cache_seed: None,
                    stream,
                })
            }
            DownstreamSurface::ChatCompletions => {
                unreachable!("Chat Completions surface is unused for Anthropic upstream")
            }
            DownstreamSurface::Models => models_surface_unreachable(),
        }
    }

    fn decode_kind(&self) -> UpstreamDecode {
        UpstreamDecode::AnthropicMessages
    }

    fn recovery(&self) -> RecoveryPolicy {
        RecoveryPolicy::None
    }
}
