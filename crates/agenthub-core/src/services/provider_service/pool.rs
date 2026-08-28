//! Provider pool CRUD, identity merge, and current-row commits.

use std::time::Instant;

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::error::{AppError, Result};
use crate::models::{
    attach_persisted_surface, Account, AgentId, PersistedTicketSurface, Provider, ProviderInput,
    TicketSurface,
};
use crate::services::provider_identity::{provider_identity, stamp_secret_hash};
use crate::services::switch_undo::{extract_probe_url, probe_url_latency_ms};
use crate::services::AdapterRouteService;
use crate::storage::{
    account_get_by_id_conn, account_list_for_agent_conn, provider_get_by_id_conn,
    provider_list_for_agent_conn, AdapterProfileRepo,
};

use super::{
    log_provider_op, now_ts, sort_providers, validate_id, validate_provider_input,
    ProviderLiveSagaGuard, ProviderService,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProviderBindingSnapshot {
    pub(super) agent_key: String,
    pub(super) account_id: Option<String>,
    pub(super) provider_id: Option<String>,
    pub(super) model_id: Option<String>,
    pub(super) config_profile_id: Option<String>,
    pub(super) revision: i64,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

#[derive(Clone, Default)]
pub(super) struct ProviderMutationFootprint {
    pub(super) affected_provider_ids: Vec<String>,
    pub(super) before_providers: Vec<Provider>,
    pub(super) after_providers: Vec<Provider>,
    pub(super) before_accounts: Vec<Account>,
    pub(super) after_accounts: Vec<Account>,
    pub(super) before_binding: Option<ProviderBindingSnapshot>,
    pub(super) after_binding: Option<ProviderBindingSnapshot>,
    pub(super) target_was_new: bool,
}

pub(super) struct ProviderCommittedMutation {
    pub(super) stored: Provider,
    pub(super) footprint: ProviderMutationFootprint,
}

impl ProviderService {
    /// Deterministic list: [`AgentId::ALL`] order, then name, then id.
    pub fn list(&self, agent: Option<AgentId>) -> Result<Vec<Provider>> {
        self.connections.reconcile_known_agents(agent);
        if let Some(agent) = agent {
            let _ = self.heal_secret_url_duplicates(agent);
        } else {
            for agent in AgentId::ALL {
                let _ = self.heal_secret_url_duplicates(agent);
            }
        }
        let mut items = self.repo.list(agent)?;
        sort_providers(&mut items);
        Ok(items)
    }

    /// Resolve by primary key id first; otherwise by exact name.
    ///
    /// - Missing → [`AppError::NotFound`]
    /// - Multiple name matches → [`AppError::InvalidArg`] (ambiguous)
    /// - Optional `agent` scopes both id and name lookup
    pub fn get(&self, id_or_name: &str, agent: Option<AgentId>) -> Result<Provider> {
        let key = id_or_name.trim();
        if key.is_empty() {
            return Err(AppError::InvalidArg(
                "provider id or name must not be empty".into(),
            ));
        }

        if let Some(p) = self.repo.get_by_id(key)? {
            if let Some(agent) = agent {
                if p.agent_id != agent {
                    return Err(AppError::NotFound(format!(
                        "provider not found: {key} (agent filter: {})",
                        agent.as_str()
                    )));
                }
            }
            return Ok(p);
        }

        let matches = self.repo.list_by_name(key, agent)?;
        match matches.len() {
            0 => Err(AppError::NotFound(format!("provider not found: {key}"))),
            1 => Ok(matches.into_iter().next().expect("len 1")),
            n => Err(AppError::InvalidArg(format!(
                "ambiguous provider name '{key}': found {n} providers; specify --agent or use id"
            ))),
        }
    }

    /// Load a pool row by primary key. Missing is `Ok(None)`; no name fallback.
    pub fn get_by_id(&self, id: &str) -> Result<Option<Provider>> {
        self.repo.get_by_id(id)
    }

    /// The unique current provider for `agent`, if any.
    pub fn get_current(&self, agent: AgentId) -> Result<Option<Provider>> {
        self.repo.get_current(agent)
    }

    /// Create a new provider. Core owns timestamps.
    ///
    /// Duplicate id → [`AppError::InvalidArg`].
    pub fn create(&self, input: &ProviderInput) -> Result<Provider> {
        let started = Instant::now();
        let agent = input.agent_id;
        let result = (|| {
            let guard = self.begin_live_saga(agent)?;
            self.create_with_guard(&guard, input)
        })();
        log_provider_op("create", agent, started, &result);
        result
    }

    /// Create a provider while an existing per-agent saga guard remains held.
    pub fn create_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        input: &ProviderInput,
    ) -> Result<Provider> {
        self.validate_live_saga_guard(guard, input.agent_id)?;
        self.create_inner(input)
    }

    pub(super) fn create_inner(&self, input: &ProviderInput) -> Result<Provider> {
        validate_provider_input(input)?;
        let now = now_ts();
        let row = Provider {
            id: input.id.clone(),
            agent_id: input.agent_id,
            name: input.name.clone(),
            settings_config: input.settings_config.clone(),
            meta: input.meta.clone(),
            is_current: input.is_current,
            created_at: now.clone(),
            updated_at: now,
        };
        let row = self.prepare_provider_surface(row)?;
        let created = if row.is_current {
            let (created, _binding) = self.connections.create_and_activate_provider(&row)?;
            created
        } else {
            self.repo.create(&row)?
        };
        self.resolve_after_identity_heal(created)
    }

    /// Update an existing provider by id. Core owns `updated_at`; preserves `created_at`.
    ///
    /// - Missing → [`AppError::NotFound`]
    /// - `agent_id` change → [`AppError::InvalidArg`]
    pub fn update(&self, input: &ProviderInput) -> Result<Provider> {
        let started = Instant::now();
        let agent = input.agent_id;
        let result = (|| {
            let guard = self.begin_live_saga(agent)?;
            self.update_with_guard(&guard, input)
        })();
        log_provider_op("update", agent, started, &result);
        result
    }

    /// Update a provider while an existing per-agent saga guard remains held.
    pub fn update_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        input: &ProviderInput,
    ) -> Result<Provider> {
        self.validate_live_saga_guard(guard, input.agent_id)?;
        self.update_and_snapshot(guard, input)
    }

    /// Persist a provider row and its active binding without writing live config.
    ///
    /// Adapter-apply compensation uses this so a later live-restore failure cannot
    /// roll the pool snapshot back through [`Self::update_with_guard`].
    pub(crate) fn update_pool_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        input: &ProviderInput,
    ) -> Result<Provider> {
        self.validate_live_saga_guard(guard, input.agent_id)?;
        self.update_inner(input)
    }

    fn update_and_snapshot(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        input: &ProviderInput,
    ) -> Result<Provider> {
        let live_guard = guard.as_live_write_guard();
        let live_saga = self.prepare_current_provider_live(
            live_guard,
            input.agent_id,
            input.is_current,
            &format!("before applying current provider {}", input.id),
        )?;
        // Pre-commit errors (validation, missing row, revision conflict) never
        // compensate: a concurrent writer may already own the scoped rows.
        let committed = self.commit_provider_mutation(input, false)?;
        if let Some((adapter, live_before)) = live_saga {
            self.apply_current_provider_live_committed(&committed, adapter, live_before)?;
        } else {
            self.sync_current_provider_live(
                live_guard,
                &committed.stored,
                "after provider update",
            )?;
        }
        Ok(committed.stored)
    }

    pub(super) fn update_inner(&self, input: &ProviderInput) -> Result<Provider> {
        Ok(self.commit_provider_mutation(input, false)?.stored)
    }

    /// Insert or update. On existing rows: preserve `created_at`, reject `agent_id` change.
    pub fn upsert(&self, input: &ProviderInput) -> Result<Provider> {
        let started = Instant::now();
        let agent = input.agent_id;
        let result = (|| {
            let guard = self.begin_live_saga(agent)?;
            self.upsert_with_guard(&guard, input)
        })();
        log_provider_op("upsert", agent, started, &result);
        result
    }

    /// Upsert a provider while an existing per-agent saga guard remains held.
    pub fn upsert_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        input: &ProviderInput,
    ) -> Result<Provider> {
        self.validate_live_saga_guard(guard, input.agent_id)?;
        self.upsert_and_snapshot(guard, input)
    }

    fn upsert_and_snapshot(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        input: &ProviderInput,
    ) -> Result<Provider> {
        let live_guard = guard.as_live_write_guard();
        let live_saga = self.prepare_current_provider_live(
            live_guard,
            input.agent_id,
            input.is_current,
            &format!("before applying current provider {}", input.id),
        )?;
        let committed = self.commit_provider_mutation(input, true)?;
        if let Some((adapter, live_before)) = live_saga {
            self.apply_current_provider_live_committed(&committed, adapter, live_before)?;
        } else {
            self.sync_current_provider_live(
                live_guard,
                &committed.stored,
                "after provider upsert",
            )?;
        }
        self.resolve_after_identity_heal(committed.stored)
    }

    // Referenced only from provider_service `tests.rs`.
    #[allow(dead_code)]
    fn upsert_inner(&self, input: &ProviderInput) -> Result<Provider> {
        Ok(self.commit_provider_mutation(input, true)?.stored)
    }

    fn commit_provider_mutation(
        &self,
        input: &ProviderInput,
        upsert: bool,
    ) -> Result<ProviderCommittedMutation> {
        validate_provider_input(input)?;
        let now = now_ts();
        // Surface classification may read adapter_profiles; do it before
        // BEGIN IMMEDIATE so we never re-enter the database mutex.
        let prepared = self.prepare_provider_surface(Provider {
            id: input.id.clone(),
            agent_id: input.agent_id,
            name: input.name.clone(),
            settings_config: input.settings_config.clone(),
            meta: input.meta.clone(),
            is_current: input.is_current,
            created_at: now.clone(),
            updated_at: now.clone(),
        })?;
        // Projection rows are not tickets. Keep the classification result from
        // `prepare_provider_surface` but do not restore an old surface below
        // when a surface-less projection updates an existing row.
        let generated_projection = self.is_generated_projection(&prepared)?;
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let agent = input.agent_id;
            let existing = match provider_get_by_id_conn(&tx, &input.id)? {
                Some(existing) if existing.agent_id != input.agent_id => {
                    return Err(AppError::InvalidArg(format!(
                        "provider agent_id is immutable (id={}, existing={}, requested={})",
                        input.id,
                        existing.agent_id.as_str(),
                        input.agent_id.as_str()
                    )));
                }
                Some(existing) => Some(existing),
                None if upsert => None,
                None => {
                    return Err(AppError::NotFound(format!(
                        "provider not found: {}",
                        input.id
                    )));
                }
            };
            let providers = provider_list_for_agent_conn(&tx, agent)?;
            let accounts = account_list_for_agent_conn(&tx, agent)?;
            let binding = get_provider_binding_row(&tx, agent)?;
            let target_was_new = existing.is_none();
            let expected_updated_at = existing.as_ref().map(|row| row.updated_at.clone());
            freeze_provider_mutation_plan(
                &tx,
                "target",
                &input.id,
                expected_updated_at.as_deref().unwrap_or(""),
            )?;
            if let Some(existing) = &existing {
                let live = provider_get_by_id_conn(&tx, &existing.id)?.ok_or_else(|| {
                    AppError::NotFound(format!("provider not found: {}", existing.id))
                })?;
                if live.updated_at != existing.updated_at {
                    return Err(AppError::message(
                        "provider.merge.conflict",
                        format!("provider changed after update snapshot: {}", existing.id),
                    ));
                }
            }

            let mut row = prepared.clone();
            if let Some(existing) = &existing {
                if !generated_projection {
                    preserve_existing_provider_surface(input, existing, &mut row);
                }
                row.created_at = existing.created_at.clone();
            }
            let stored = if row.is_current {
                self.connections
                    .activate_provider_if_revision_conn(&tx, &row, expected_updated_at.as_deref())?
                    .0
            } else {
                self.connections
                    .store_provider_non_current_if_revision_conn(
                        &tx,
                        &row,
                        expected_updated_at.as_deref(),
                    )?
            };

            let mut affected_provider_ids = vec![input.id.clone()];
            if stored.is_current {
                for row in providers.iter().filter(|row| row.is_current) {
                    if !affected_provider_ids.iter().any(|id| id == &row.id) {
                        affected_provider_ids.push(row.id.clone());
                    }
                }
            }
            let before_providers = affected_provider_ids
                .iter()
                .filter_map(|id| providers.iter().find(|row| row.id == *id).cloned())
                .collect::<Vec<_>>();
            let after_providers = affected_provider_ids
                .iter()
                .filter_map(|id| provider_get_by_id_conn(&tx, id).ok().flatten())
                .collect::<Vec<_>>();
            let before_accounts = if stored.is_current {
                accounts
                    .iter()
                    .filter(|row| row.is_current)
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let after_accounts = before_accounts
                .iter()
                .filter_map(|row| account_get_by_id_conn(&tx, &row.id).ok().flatten())
                .collect::<Vec<_>>();
            let after_binding = get_provider_binding_row(&tx, agent)?;
            tx.commit()?;
            Ok(ProviderCommittedMutation {
                stored,
                footprint: ProviderMutationFootprint {
                    affected_provider_ids,
                    before_providers,
                    after_providers,
                    before_accounts,
                    after_accounts,
                    before_binding: binding,
                    after_binding,
                    target_was_new,
                },
            })
        })
    }

    /// Delete by primary key id.
    ///
    /// - Empty / invalid id → [`AppError::InvalidArg`]
    /// - Missing → [`AppError::NotFound`]
    pub fn delete(&self, id: &str, agent: AgentId) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let guard = self.begin_live_saga(agent)?;
            self.delete_with_guard(&guard, id, agent)
        })();
        log_provider_op("delete", agent, started, &result);
        result
    }

    /// Delete a provider while an existing per-agent saga guard remains held.
    pub fn delete_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        id: &str,
        agent: AgentId,
    ) -> Result<()> {
        self.validate_live_saga_guard(guard, agent)?;
        self.delete_inner(id, agent)
    }

    pub(super) fn delete_inner(&self, id: &str, agent: AgentId) -> Result<()> {
        validate_id(id)?;
        // Clear active binding in the same transaction when deleting the active row.
        self.connections.delete_provider(id, agent)
    }

    pub(super) fn resolve_after_identity_heal(&self, stored: Provider) -> Result<Provider> {
        self.heal_secret_url_duplicates(stored.agent_id)?;
        if self.repo.get_by_id(&stored.id)?.is_some() {
            return self.repo.get_by_id(&stored.id)?.ok_or_else(|| {
                AppError::message("db.provider", "provider missing after identity heal")
            });
        }
        let Some(identity) = provider_identity(&stored) else {
            return Ok(stored);
        };
        let keeper = self
            .repo
            .list(Some(stored.agent_id))?
            .into_iter()
            .find(|row| provider_identity(row).as_ref() == Some(&identity));
        keeper.ok_or_else(|| {
            AppError::NotFound(format!(
                "provider not found after identity heal: {}",
                stored.id
            ))
        })
    }

    /// Merge same-agent rows that share secret hash + base URL into one keeper.
    /// Losers go to the recovery bin with a recycle log. Cursor is left alone.
    pub(super) fn heal_secret_url_duplicates(&self, agent: AgentId) -> Result<()> {
        if agent == AgentId::Cursor {
            return Ok(());
        }
        use std::collections::HashMap;

        use crate::services::provider_identity::{
            pick_identity_keeper, retarget_profiles_from_loser, ProviderIdentity,
        };

        let rows = self.repo.list(Some(agent))?;
        let profile_repo = AdapterProfileRepo::new(self.db.clone());
        let mut profiles = profile_repo.list_filtered(&Default::default())?;
        let mut groups: HashMap<ProviderIdentity, Vec<Provider>> = HashMap::new();
        for row in rows {
            let Some(identity) = provider_identity(&row) else {
                continue;
            };
            groups.entry(identity).or_default().push(row);
        }
        for group in groups.into_values() {
            if group.len() < 2 {
                continue;
            }
            let Some(keeper) = pick_identity_keeper(&group, &profiles).cloned() else {
                continue;
            };
            for loser in group.iter().filter(|row| row.id != keeper.id) {
                let changed = retarget_profiles_from_loser(&mut profiles, &loser.id, &keeper.id);
                for index in changed {
                    let _ = profile_repo.update(&profiles[index]);
                }
                tracing::info!(
                    module = crate::logging::targets::PROVIDER,
                    op = "recycle",
                    agent = agent.as_str(),
                    id = loser.id.as_str(),
                    name = loser.name.as_str(),
                    keeper_id = keeper.id.as_str(),
                    "merged duplicate login into existing row"
                );
                self.connections.delete_provider(&loser.id, agent)?;
            }
        }
        Ok(())
    }

    /// Add `meta.surface` to a prospective provider before its first database
    /// mutation. Adapter-generated projections are not tickets.
    pub(super) fn prepare_provider_surface(&self, mut provider: Provider) -> Result<Provider> {
        if self.is_generated_projection(&provider)? {
            return Ok(provider);
        }
        if TicketSurface::from_persisted_json(&provider.meta) == PersistedTicketSurface::Missing {
            let surface = Self::classify_persisted_provider_surface(&provider);
            attach_persisted_surface(&mut provider.meta, surface);
        }
        stamp_secret_hash(&mut provider.meta, &provider.settings_config);
        Ok(provider)
    }

    /// Keep an existing import row untouched when the live snapshot itself did
    /// not create a new provider. Surface classification belongs only to the
    /// create path; this also keeps legacy Missing rows stable on import no-ops.
    pub(super) fn stamp_provider_surface(&self, provider: Provider) -> Result<Provider> {
        Ok(provider)
    }

    /// Projections are not tickets. Match `generatedBy=adapter` or an existing
    /// profile that already points at this row as `generated_provider_id`.
    pub(super) fn is_generated_projection(&self, provider: &Provider) -> Result<bool> {
        if provider
            .meta
            .get("generatedBy")
            .and_then(|value| value.as_str())
            == Some("adapter")
        {
            return Ok(true);
        }
        Ok(AdapterProfileRepo::new(self.db.clone())
            .list_filtered(&Default::default())?
            .iter()
            .any(|profile| profile.generated_provider_id.as_deref() == Some(provider.id.as_str())))
    }

    /// Persist a provider surface only when an OpenAI classification has evidence
    /// of the official product. The route classifier also recognizes arbitrary
    /// OpenAI-compatible remotes for adapter planning, but a relay alone is not a
    /// product identity for the provider ticket surface.
    pub(super) fn classify_persisted_provider_surface(provider: &Provider) -> TicketSurface {
        let product = AdapterRouteService::classify_provider_source_product(provider);
        let surface = TicketSurface::from_product(product);
        if surface != TicketSurface::OpenaiApi || Self::provider_proves_openai_api(provider) {
            surface
        } else {
            TicketSurface::Unknown
        }
    }

    pub(super) fn provider_proves_openai_api(provider: &Provider) -> bool {
        crate::services::adapter_route_constants::provider_has_official_openai_api_evidence(
            provider,
        )
    }

    /// Best-effort TCP/HTTP reachability probe of a saved provider base URL.
    ///
    /// Returns round-trip milliseconds when the endpoint answers (any HTTP status
    /// counts as reachable). Missing URL or network failure → error.
    pub fn test_latency(&self, agent: AgentId, provider_id: &str) -> Result<u64> {
        let provider = self.get(provider_id, Some(agent))?;
        let url = extract_probe_url(&provider.settings_config).ok_or_else(|| {
            AppError::InvalidArg(
                "该连接没有可探测的 Base URL（base_url / ANTHROPIC_BASE_URL 等）".into(),
            )
        })?;
        probe_url_latency_ms(&url)
    }
}

pub(super) fn get_provider_binding_row(
    conn: &Connection,
    agent: AgentId,
) -> Result<Option<ProviderBindingSnapshot>> {
    let key = crate::platform::AgentKey::from_agent_id(agent).into_string();
    conn.query_row(
        r#"
        SELECT agent_key, account_id, provider_id, model_id, config_profile_id,
               revision, created_at, updated_at
        FROM agent_active_bindings WHERE agent_key = ?1
        "#,
        params![key],
        |row| {
            Ok(ProviderBindingSnapshot {
                agent_key: row.get(0)?,
                account_id: row.get(1)?,
                provider_id: row.get(2)?,
                model_id: row.get(3)?,
                config_profile_id: row.get(4)?,
                revision: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(AppError::from)
}

pub(super) fn freeze_provider_mutation_plan(
    tx: &Transaction<'_>,
    role: &str,
    id: &str,
    expected_updated_at: &str,
) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS provider_mutation_plan (
            role TEXT NOT NULL,
            id TEXT NOT NULL,
            expected_updated_at TEXT NOT NULL
        );
        DELETE FROM provider_mutation_plan;
        "#,
    )?;
    tx.execute(
        "INSERT INTO provider_mutation_plan (role, id, expected_updated_at) VALUES (?1, ?2, ?3)",
        params![role, id, expected_updated_at],
    )?;
    Ok(())
}

/// Keep persisted surface metadata authoritative when a surface-less caller
/// updates an existing provider. The UI input intentionally omits this field;
/// classifying that input before the transaction is only safe for a new row.
pub(super) fn preserve_existing_provider_surface(
    input: &ProviderInput,
    existing: &Provider,
    prepared: &mut Provider,
) {
    if input.meta.get("surface").is_some() {
        return;
    }

    let Some(meta) = prepared.meta.as_object_mut() else {
        return;
    };
    if let Some(surface) = existing.meta.get("surface") {
        meta.insert("surface".into(), surface.clone());
    } else {
        // `prepare_provider_surface` may have classified the prospective row
        // before the transaction. Existing rows with no key stay Missing.
        meta.remove("surface");
    }
}
