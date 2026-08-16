//! Test-only ninth agent (`demo-agent`).
//!
//! Never compiled into production `register_integrations`. Adding this agent
//! is one directory plus one `register` call — no platform service / page
//! branches and no `AgentId` variant.

mod config;
mod detect;
mod install;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::integrations::IntegrationContext;
use crate::models::{Capability, CapabilityLevel, CapabilityStateDto, RuntimeId};
use crate::platform::agent_catalog::{AgentDescriptor, AgentKey, InstallChannelDescriptor};

pub const KEY: &str = "demo-agent";

pub fn key() -> AgentKey {
    AgentKey::parse(KEY).expect("demo-agent is a valid open AgentKey")
}

/// Register sparse demo-agent ports into an injectable context.
pub fn register(ctx: &mut IntegrationContext<'_>, installed: Arc<AtomicBool>) {
    detect::register(ctx, Arc::clone(&installed));
    install::register(ctx);
    config::register(ctx);
}

pub fn descriptor() -> AgentDescriptor {
    let mut capabilities = std::collections::BTreeMap::new();
    for cap in Capability::ALL {
        let (level, reason) = match cap {
            Capability::ConfigWrite => (CapabilityLevel::Full, None),
            Capability::Usage
            | Capability::Mcp
            | Capability::ModelSelect
            | Capability::SessionResume => (
                CapabilityLevel::Planned,
                Some("demo-agent roadmap cell".into()),
            ),
            _ => (
                CapabilityLevel::Unsupported,
                Some("demo-agent sparse surface".into()),
            ),
        };
        capabilities.insert(
            cap.as_str().to_string(),
            CapabilityStateDto {
                level,
                reason,
                min_version: None,
            },
        );
    }
    AgentDescriptor {
        key: key(),
        display_name: "Demo Agent".into(),
        integration_version: 1,
        capabilities,
        install_channels: vec![InstallChannelDescriptor {
            id: "npm".into(),
            label: "npm".into(),
            command: "npm i -g @agenthub/demo-agent".into(),
            requires: vec![RuntimeId::NodeJs],
        }],
        config_schema_version: Some(1),
    }
}
