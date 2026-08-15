//! Agent catalog aggregation service (read-only).

use std::collections::{BTreeMap, HashMap};

use crate::adapters::AdapterRegistry;
use crate::platform::install::{channels_for, list_install_catalog};
use crate::error::{AppError, Result};
use crate::models::{AgentId, Capability, CapabilityStateDto};

use super::{AgentDescriptor, AgentKey, InstallChannelDescriptor};

/// Current integration version for built-in adapters migrated via legacy registry.
const BUILTIN_INTEGRATION_VERSION: u32 = 1;

/// Read-only catalog of agent descriptors.
///
/// # Aggregation bridge (temporary)
///
/// TODO(P03-P08): build descriptors from `integrations/agents/*` modules and
/// sparse ports instead of `AgentId::ALL` + [`AdapterRegistry`] + install
/// catalog match arms. Remove `from_registry` bridge once CodeGraph shows no
/// platform callers depend on those legacy sources for discovery.
#[derive(Debug, Clone)]
pub struct AgentCatalogService {
    /// Product order (stable; locked by tests).
    descriptors: Vec<AgentDescriptor>,
    index: HashMap<String, usize>,
}

impl AgentCatalogService {
    /// Build a catalog from an explicit descriptor list (test + future inject).
    ///
    /// Order is preserved. Keys must be unique and already-validated.
    pub fn new(descriptors: Vec<AgentDescriptor>) -> Result<Self> {
        let mut index = HashMap::with_capacity(descriptors.len());
        for (i, d) in descriptors.iter().enumerate() {
            let key = d.key.as_str().to_string();
            if index.insert(key.clone(), i).is_some() {
                return Err(AppError::InvalidArg(format!(
                    "duplicate agent key in catalog: {key}"
                )));
            }
        }
        Ok(Self { descriptors, index })
    }

    /// Aggregate from the live adapter registry + install catalog.
    ///
    /// Order = [`AgentId::ALL`] (product order). Missing adapters fail closed.
    pub fn from_registry(registry: &AdapterRegistry) -> Result<Self> {
        // Temporary bridge: product order still follows closed AgentId::ALL.
        // TODO(P01→P13): drop AgentId::ALL once registry keys are open strings.
        let install_by_agent: BTreeMap<AgentId, Vec<InstallChannelDescriptor>> =
            list_install_catalog()
                .into_iter()
                .map(|entry| {
                    let channels = entry
                        .channels
                        .into_iter()
                        .map(|ch| InstallChannelDescriptor {
                            id: ch.id,
                            label: ch.label,
                            command: ch.command,
                            requires: ch.requires,
                        })
                        .collect();
                    (entry.agent_id, channels)
                })
                .collect();

        let mut descriptors = Vec::with_capacity(AgentId::ALL.len());
        for id in AgentId::ALL {
            let adapter = registry.get(id).ok_or_else(|| {
                AppError::NotFound(format!(
                    "adapter not registered for catalog: {}",
                    id.as_str()
                ))
            })?;

            let mut capabilities = BTreeMap::new();
            for cap in Capability::ALL {
                let state = adapter.capability(cap);
                capabilities.insert(cap.as_str().to_string(), CapabilityStateDto::from(state));
            }

            let install_channels = install_by_agent.get(&id).cloned().unwrap_or_else(|| {
                // Fallback should be unreachable when install catalog covers ALL;
                // keep channels_for so a partial catalog still hydrates.
                channels_for(id)
                    .into_iter()
                    .map(|ch| InstallChannelDescriptor {
                        id: ch.id,
                        label: ch.label,
                        command: ch.command,
                        requires: ch.requires,
                    })
                    .collect()
            });

            // Config schema version from platform projectors (P08); unsupported = None.
            let config_schema_version = crate::platform::config::builtin_config_registry()
                .get_agent_id(id)
                .map(|p| p.schema().schema_version);

            descriptors.push(AgentDescriptor {
                key: AgentKey::from_agent_id(id),
                display_name: id.display_name().to_string(),
                integration_version: BUILTIN_INTEGRATION_VERSION,
                capabilities,
                install_channels,
                config_schema_version,
            });
        }

        Self::new(descriptors)
    }

    /// Catalog for the built-in `register_all()` registry.
    pub fn builtin() -> Result<Self> {
        Self::from_registry(&crate::adapters::register_all())
    }

    /// Deterministic list (product order for builtin; insert order for [`Self::new`]).
    pub fn list(&self) -> &[AgentDescriptor] {
        &self.descriptors
    }

    /// Owned clone of the full list (Tauri / JSON boundaries).
    pub fn list_owned(&self) -> Vec<AgentDescriptor> {
        self.descriptors.clone()
    }

    /// Lookup by validated key. Unknown keys → [`AppError::NotFound`] (no fallback).
    pub fn get(&self, key: &AgentKey) -> Result<&AgentDescriptor> {
        self.index
            .get(key.as_str())
            .map(|&i| &self.descriptors[i])
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "agent not found in catalog: {} (unavailable)",
                    key.as_str()
                ))
            })
    }

    /// Parse raw string then lookup. Invalid format → `invalid_arg`; missing → `not_found`.
    pub fn get_str(&self, key: &str) -> Result<&AgentDescriptor> {
        let key = AgentKey::parse(key)?;
        self.get(&key)
    }

    pub fn contains(&self, key: &AgentKey) -> bool {
        self.index.contains_key(key.as_str())
    }

    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}
