use axum::response::Response;
use reqwest::RequestBuilder;

use crate::bridge::grok_cli::GrokCliRequestIdentity;
use crate::bridge::protocol::responses::to_kimi_chat_request;

use super::super::admission::AdmittedRequest;
use super::super::surface::DownstreamSurface;
use super::{
    models_surface_unreachable, overwrite_configured_model_with, parse_bridge_request, RecoveryPolicy,
    UpstreamDecode, UpstreamPrepare, UpstreamTransport,
};

pub(super) struct OpenAiChatTransport;

impl UpstreamTransport for OpenAiChatTransport {
    fn path(&self) -> &'static str {
        "chat/completions"
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
            DownstreamSurface::Responses | DownstreamSurface::Messages => {
                let request = parse_bridge_request(surface, admitted)?;
                let stream = request.stream;
                let mut body = to_kimi_chat_request(&request);
                overwrite_configured_model_with(
                    &mut body,
                    admitted.state.upstream.model.as_deref(),
                    admitted.state.custom_openai,
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
                unreachable!(
                    "Chat Completions surface is unreachable for OpenAI Chat Completions upstream"
                )
            }
            DownstreamSurface::Models => models_surface_unreachable(),
        }
    }

    fn decode_kind(&self) -> UpstreamDecode {
        UpstreamDecode::ChatCompletions
    }

    fn recovery(&self) -> RecoveryPolicy {
        RecoveryPolicy::None
    }
}
