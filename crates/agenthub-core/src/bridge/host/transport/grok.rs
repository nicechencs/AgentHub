use axum::response::Response;
use reqwest::RequestBuilder;
use serde_json::Value;

use crate::bridge::grok_cli::{
    apply_grok_cli_identity_with, extract_prompt_cache_seed, grok_cli_request_identity_for_account,
    inject_prompt_cache_key, normalize_grok_build_tools, GrokCliRequestIdentity,
};
use crate::bridge::protocol::responses::to_grok_responses_request;

use super::super::admission::AdmittedRequest;
use super::super::surface::DownstreamSurface;
use super::{
    models_surface_unreachable, overwrite_configured_model, parse_bridge_request,
    passthrough_responses_object, RecoveryPolicy, UpstreamDecode, UpstreamPrepare,
    UpstreamTransport,
};

pub(super) struct GrokTransport;

impl UpstreamTransport for GrokTransport {
    fn path(&self) -> &'static str {
        "responses"
    }

    fn apply_auth(
        &self,
        builder: RequestBuilder,
        token: &str,
        grok_identity: Option<&GrokCliRequestIdentity>,
    ) -> RequestBuilder {
        apply_grok_cli_identity_with(builder.bearer_auth(token), grok_identity)
    }

    fn prepare(
        &self,
        surface: DownstreamSurface,
        admitted: &AdmittedRequest,
    ) -> Result<UpstreamPrepare, Response> {
        match surface {
            DownstreamSurface::Responses => {
                let grok_identity = grok_identity(admitted);
                let cache_seed = extract_prompt_cache_seed(&admitted.headers, &admitted.body);
                let (mut body, stream) = passthrough_responses_object(admitted.body.clone())?;
                if let Some(model) = &admitted.state.upstream.model {
                    if !model.trim().is_empty() {
                        body["model"] = Value::String(model.clone());
                    }
                }
                normalize_grok_build_tools(&mut body);
                inject_prompt_cache_key(&mut body, cache_seed.as_deref());
                Ok(UpstreamPrepare {
                    path: self.path(),
                    body,
                    grok_identity: Some(grok_identity),
                    cache_seed,
                    stream,
                })
            }
            DownstreamSurface::Messages => {
                let grok_identity = grok_identity(admitted);
                let cache_seed = extract_prompt_cache_seed(&admitted.headers, &admitted.body);
                let request = parse_bridge_request(surface, admitted)?;
                let stream = request.stream;
                let mut body = to_grok_responses_request(&request);
                inject_prompt_cache_key(&mut body, cache_seed.as_deref());
                overwrite_configured_model(&mut body, admitted.state.upstream.model.as_deref(), &admitted.state.listed_models);
                Ok(UpstreamPrepare {
                    path: self.path(),
                    body,
                    grok_identity: Some(grok_identity),
                    cache_seed,
                    stream,
                })
            }
            DownstreamSurface::ChatCompletions => {
                unreachable!("Chat Completions surface is unreachable for Grok upstream")
            }
            DownstreamSurface::Models => models_surface_unreachable(),
        }
    }

    fn decode_kind(&self) -> UpstreamDecode {
        UpstreamDecode::OpenAiResponses
    }

    fn recovery(&self) -> RecoveryPolicy {
        RecoveryPolicy::Oauth401ReloadAndGrokReasoning
    }
}

fn grok_identity(admitted: &AdmittedRequest) -> GrokCliRequestIdentity {
    let account_id = admitted
        .member
        .as_ref()
        .and_then(|member| admitted.state.account_picker.partition_account_id(member));
    grok_cli_request_identity_for_account(
        &admitted.request_id,
        &admitted.headers,
        &admitted.body,
        admitted.state.upstream.model.as_deref(),
        account_id,
    )
}
