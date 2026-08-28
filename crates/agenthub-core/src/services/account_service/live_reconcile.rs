use serde_json::json;
use uuid::Uuid;

use crate::adapters::AgentAdapter;
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    Account, AccountKind, AdapterProfile, AdapterProfileFilter, AgentId, AuthState, BackupKind,
    Capability, LiveAccount, Provider,
};
use crate::services::adapter_projection::{
    classify_account_live, leftover_live_flag, should_skip_live_reconcile, LiveOrigin,
};
use crate::storage::{AdapterProfileRepo, ProviderRepo};

use super::surface::*;
use super::{AccountService, MAX_ACCOUNT_LABEL_LEN};

impl AccountService {
    pub(super) fn sync_current_live(&self, agent: Option<AgentId>) {
        let adapters = match agent {
            Some(id) => self.registry.get(id).into_iter().collect::<Vec<_>>(),
            None => self.registry.all(),
        };
        for adapter in adapters {
            let id = adapter.id();
            if !adapter.capability(Capability::AccountSwitch).is_usable() {
                continue;
            }
            // Keep the snapshot, match, and persistence decision serialized with
            // account switches. The process-local guard is still required for
            // CRUD-only services that have no file-lock directory configured.
            let process_lock = live_reconcile_lock(id);
            let _process_lock = process_lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let _file_lock = match self.acquire_live_lock(id) {
                Ok(lock) => lock,
                Err(error) => {
                    tracing::debug!(
                        module = targets::ACCOUNT,
                        agent = id.as_str(),
                        error_code = error.code(),
                        "live account sync skipped because the live lock is held"
                    );
                    continue;
                }
            };
            let lives = match self.read_live_accounts(adapter.as_ref(), id) {
                Ok(lives) => lives,
                Err(error) if error.code() == "not_found" || error.code() == "unsupported" => {
                    continue;
                }
                Err(error) => {
                    tracing::debug!(
                        module = targets::ACCOUNT,
                        agent = id.as_str(),
                        error_code = error.code(),
                        "live account sync skipped"
                    );
                    continue;
                }
            };
            let mut grants = Vec::new();
            for live in lives {
                if self.skip_projection_reconcile(id, &live) {
                    continue;
                }
                grants.push(live);
            }
            // One live snapshot is the agent's current login. Nested Grok slots
            // are concurrent people in one file — update rows in place, then
            // align current without last-sorted-slot stealing.
            let activate_each = grants.len() <= 1;
            for live in grants.iter().cloned() {
                if let Err(error) = self.reconcile_live_account_with_activate(
                    adapter.as_ref(),
                    id,
                    live,
                    activate_each,
                ) {
                    tracing::warn!(
                        module = targets::ACCOUNT,
                        agent = id.as_str(),
                        error_code = error.code(),
                        "failed to persist live account rotation"
                    );
                }
            }
            if !activate_each {
                if let Err(error) =
                    self.align_current_after_multi_live(adapter.as_ref(), id, &grants)
                {
                    tracing::warn!(
                        module = targets::ACCOUNT,
                        agent = id.as_str(),
                        error_code = error.code(),
                        "failed to align current after multi-slot live sync"
                    );
                }
            }
            if let Err(error) = self.ensure_exclusive_live_source(adapter.as_ref(), id) {
                tracing::warn!(
                    module = targets::ACCOUNT,
                    agent = id.as_str(),
                    error_code = error.code(),
                    "failed to make the current login the only live source"
                );
            }
        }
    }

    /// Read the live account slots represented by an adapter snapshot. Pi's
    /// auth.json and Grok's nested OAuth profiles are combined file snapshots,
    /// so they must be expanded before they reach pool reconciliation; the
    /// combined snapshot is only safe for backup / complete-file rollback.
    pub(super) fn read_live_accounts(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
    ) -> Result<Vec<LiveAccount>> {
        let snapshot = adapter.read_account()?;
        if snapshot.agent != agent {
            return Err(AppError::InvalidArg(format!(
                "adapter returned account for {}, expected {}",
                snapshot.agent.as_str(),
                agent.as_str()
            )));
        }
        if live_account_is_empty(&snapshot) {
            return Err(AppError::NotFound(
                "no live account credentials found".into(),
            ));
        }
        if agent == AgentId::Pi {
            let body = snapshot.credentials.get("body").ok_or_else(|| {
                AppError::InvalidArg("Pi combined live account is missing credentials.body".into())
            })?;
            return crate::adapters::pi_auth::expand_auth_to_live_accounts(body);
        }
        if agent == AgentId::Grok {
            return Ok(crate::adapters::expand_grok_auth_to_live_accounts(
                &snapshot,
            ));
        }
        if agent == AgentId::Kimi {
            return Ok(crate::adapters::expand_kimi_live_accounts(&snapshot));
        }
        if agent == AgentId::Claude {
            return Ok(crate::adapters::expand_claude_live_accounts(&snapshot));
        }
        Ok(vec![snapshot])
    }

    /// Reconcile one safe live snapshot into the account pool.
    ///
    /// Exact tokens match first. Same-agent OAuth identity overwrites one row
    /// and collapses leftover same-identity rows. Unknown identity stays
    /// fail-closed except for identical credentials.
    pub(super) fn reconcile_live_account(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        live: LiveAccount,
    ) -> Result<Option<Account>> {
        self.reconcile_live_account_with_activate(adapter, agent, live, true)
    }

    pub(super) fn reconcile_live_account_with_activate(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        live: LiveAccount,
        activate: bool,
    ) -> Result<Option<Account>> {
        if live.agent != agent || live_account_is_empty(&live) {
            return Ok(None);
        }
        if self.skip_projection_reconcile(agent, &live) {
            return Ok(None);
        }
        let rows = self.repo.list(Some(agent))?;

        let matches = authorization_duplicates(adapter, agent, live.kind, &live.credentials, &rows);
        let match_count = matches.len();
        if let Some(existing) = pick_primary_authorization_match(matches) {
            if existing.kind == AccountKind::Oauth
                && live.kind == AccountKind::Oauth
                && super::oauth_file_sync::supports_oauth_file_sync(agent)
            {
                return self.reconcile_oauth_row_with_cli_file(
                    adapter,
                    agent,
                    existing,
                    live,
                    match_count,
                    activate,
                );
            }
            let (row, changed) = self.update_live_row(adapter, existing, live);
            if match_count > 1 {
                let mark_current = activate && agent != AgentId::Pi;
                return self
                    .commit_authorization_merge(
                        adapter,
                        &row,
                        row.kind,
                        row.label.clone(),
                        row.credentials.clone(),
                        row.extra.clone(),
                        mark_current,
                    )
                    .map(|committed| Some(committed.stored))
                    .map_err(|error| error.into_error());
            }
            return Ok(Some(
                self.persist_reconciled_live_row(agent, row, changed, activate)?,
            ));
        }

        if stable_live_identity(adapter, live.kind, &live.credentials).is_none() {
            // API-key / file snapshots often have no email/sub. Exact
            // authorization already matched above; anything else stays
            // fail-closed instead of inventing a pool row.
            tracing::debug!(
                module = targets::ACCOUNT,
                agent = agent.as_str(),
                "live account identity is unknown; refusing non-exact reconcile"
            );
            return Ok(None);
        }

        // New identity on this agent (Pi: this live slot). Make it current for
        // a single live snapshot; nested Grok people stay non-current until
        // align_current_after_multi_live picks one.
        let label = live
            .label_hint
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("{} · OAuth", agent.display_name()));
        validate_label(&label, "account label", MAX_ACCOUNT_LABEL_LEN)?;
        let mut extra = live.extra.clone();
        if let Some(obj) = extra.as_object_mut() {
            obj.insert("source".into(), json!("live"));
        }
        let extra = attach_identity_meta(adapter, live.kind, &live.credentials, &label, extra);
        let now = now_ts();
        let mark_current = activate && agent != AgentId::Pi;
        let row = Account {
            id: format!("{}-live-{}", agent.as_str(), Uuid::new_v4()),
            agent_id: agent,
            kind: live.kind,
            label: label.clone(),
            credentials: live.credentials.clone(),
            extra: extra.clone(),
            status: "active".into(),
            is_current: mark_current,
            created_at: now.clone(),
            updated_at: now,
        };
        let row = self.prepare_account_surface(row);
        let created = self
            .commit_authorization_merge(
                adapter,
                &row,
                live.kind,
                label,
                live.credentials,
                extra,
                mark_current,
            )
            .map(|committed| committed.stored)
            .map_err(|error| error.into_error())?;
        Ok(Some(created))
    }

    pub(super) fn update_live_row(
        &self,
        adapter: &dyn AgentAdapter,
        mut row: Account,
        live: LiveAccount,
    ) -> (Account, bool) {
        if !live_credentials_changed(&row, &live) {
            return (row, false);
        }
        let persisted_extra = row.extra.clone();
        let display =
            if row.kind == AccountKind::Oauth && is_generic_oauth_label(&row.label, row.agent_id) {
                live.label_hint
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| row.label.clone())
            } else {
                row.label.clone()
            };
        let mut extra = live.extra.clone();
        if let Some(obj) = extra.as_object_mut() {
            obj.insert("source".into(), json!("live"));
        }
        row.label = display.clone();
        row.credentials = live.credentials;
        row.extra = attach_identity_meta(adapter, live.kind, &row.credentials, &display, extra);
        Self::copy_persisted_surface(&persisted_extra, &mut row.extra);
        Self::copy_persisted_identity(&persisted_extra, &mut row.extra);
        row.kind = live.kind;
        row.status = "active".into();
        let _ = crate::services::account_identity_heal::heal_account_identity(&mut row);
        let _ = crate::services::account_quota::heal_token_expiry(&mut row);
        row = self.prepare_account_surface(row);
        (row, true)
    }

    /// Persist identity/quota heals. A CAS conflict means another list/heal
    /// already wrote; reload that row instead of warning on every GUI poll.
    pub(super) fn persist_healed_fields(
        &self,
        account: &Account,
        expected_updated_at: &str,
    ) -> Result<Account> {
        let updated_at = now_ts();
        match self
            .repo
            .update_healed_fields(account, expected_updated_at, &updated_at)
        {
            Ok(updated) => Ok(updated),
            Err(error) if error.code() == "account.conflict" => {
                tracing::debug!(
                    module = targets::ACCOUNT,
                    account_id = %account.id,
                    agent = account.agent_id.as_str(),
                    "heal lost the race; using latest row"
                );
                self.repo
                    .get_by_id(&account.id)?
                    .ok_or_else(|| AppError::NotFound(format!("account not found: {}", account.id)))
            }
            Err(error) => Err(error),
        }
    }

    /// Persist a reconciled row and, for single-current agents, atomically
    /// align both the legacy current flag and the active connection binding.
    /// Pi provider slots are concurrent entries in one auth.json, so they must
    /// never be globally activated by reconciliation.
    pub(super) fn persist_reconciled_live_row(
        &self,
        agent: AgentId,
        row: Account,
        changed: bool,
        activate: bool,
    ) -> Result<Account> {
        let original = row.clone();
        let expected_updated_at = row.updated_at.clone();
        let original_extra = row.extra.clone();
        let mut row = self.prepare_account_surface(row);
        let surface_changed = row.extra != original_extra;
        if surface_changed && !changed {
            return match self.stamp_account_surface(original) {
                Ok(account) => Ok(account),
                Err(error) if error.code() == "account.conflict" => self
                    .repo
                    .get_by_id(&row.id)?
                    .ok_or_else(|| AppError::NotFound(format!("account not found: {}", row.id))),
                Err(error) => Err(error),
            };
        }
        if agent == AgentId::Pi || !activate {
            return if changed {
                self.persist_healed_fields(&row, &expected_updated_at)
            } else {
                Ok(row)
            };
        }
        if !changed && row.is_current {
            return Ok(row);
        }
        if leftover_shaped_codex_live(agent) && !row.is_current {
            return if changed {
                self.persist_healed_fields(&row, &expected_updated_at)
            } else {
                Ok(row)
            };
        }
        row.is_current = true;
        row.updated_at = now_ts();
        match self
            .connections
            .update_and_activate_account(&row, &expected_updated_at)
        {
            Ok((updated, _)) => Ok(updated),
            Err(error)
                if error.code() == "account.merge.conflict"
                    || error.code() == "account.conflict" =>
            {
                self.repo
                    .get_by_id(&row.id)?
                    .ok_or_else(|| AppError::NotFound(format!("account not found: {}", row.id)))
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn pick_live_grant_to_activate(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        grants: &[LiveAccount],
        current: Option<&Account>,
    ) -> usize {
        if grants.is_empty() {
            return 0;
        }
        if grants.len() == 1 || agent != AgentId::Grok {
            return 0;
        }
        if let Some(current) = current {
            if let Some(index) = grants
                .iter()
                .position(|live| live_grant_matches_account(adapter, live, current))
            {
                return index;
            }
        }
        grants
            .iter()
            .position(crate::adapters::grok_live_uses_default_auth_slot)
            .unwrap_or(0)
    }

    pub(super) fn align_current_after_multi_live(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        grants: &[LiveAccount],
    ) -> Result<()> {
        if agent == AgentId::Pi || grants.len() <= 1 {
            return Ok(());
        }
        let rows = self.repo.list(Some(agent))?;
        if let Some(current) = rows.iter().find(|row| row.is_current) {
            if grants
                .iter()
                .any(|live| live_grant_matches_account(adapter, live, current))
            {
                return Ok(());
            }
        }
        let index = self.pick_live_grant_to_activate(adapter, agent, grants, None);
        let live = &grants[index];
        let Some(row) = rows
            .iter()
            .find(|row| live_grant_matches_account(adapter, live, row))
        else {
            return Ok(());
        };
        if row.is_current {
            return Ok(());
        }
        let now = now_ts();
        self.connections
            .activate_account(agent, &row.id, &row.updated_at, &now)?;
        Ok(())
    }

    /// Add a transient, desensitized AuthState view to the current pool row.
    /// This intentionally runs after all persistence/healing and does not call
    /// AccountRepo::update, keeping live state separate from the account pool.
    pub(super) fn merge_live_auth_state(&self, items: &mut [Account], agent: Option<AgentId>) {
        let adapters = match agent {
            Some(id) => self.registry.get(id).into_iter().collect::<Vec<_>>(),
            None => self.registry.all(),
        };
        for adapter in adapters {
            let id = adapter.id();
            let Ok(state) = adapter.read_auth() else {
                continue;
            };
            if state.agent != id {
                continue;
            }
            let Ok(lives) = self.read_live_accounts(adapter.as_ref(), id) else {
                continue;
            };
            let Some(current) = items.iter_mut().find(|row| {
                row.agent_id == id
                    && row.is_current
                    && lives.iter().any(|live| {
                        same_live_slot(id, &live.credentials, &row.credentials)
                            && row.kind == live.kind
                            && accounts_same_authorization(
                                adapter.as_ref(),
                                live.kind,
                                &live.credentials,
                                row,
                            )
                    })
            }) else {
                continue;
            };
            if !current.extra.is_object() {
                current.extra = json!({});
            }
            if let Some(extra) = current.extra.as_object_mut() {
                extra.insert("authHealth".into(), json!(state.health));
                extra.insert("authSource".into(), json!(state.source));
                extra.insert("liveRevision".into(), json!(state.revision));
            }
        }
    }

    /// Leftover / active 本机路由 live must not block switching back to 官方登录.
    pub(super) fn leftover_live_skips_identity(&self, agent: AgentId) -> bool {
        leftover_live_flag(agent) || self.live_is_adapter_projection(agent).unwrap_or(false)
    }

    /// Current oauth/API Key cannot both be live in the tool. List/boot only
    /// imported files into the pool; this writes the current login back when a
    /// leftover pointer would still win.
    fn ensure_exclusive_live_source(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
    ) -> Result<()> {
        if !adapter.capability(Capability::AccountSwitch).is_usable() {
            return Ok(());
        }
        let Some(current) = self.repo.get_current(agent)? else {
            return Ok(());
        };
        if current.kind != AccountKind::Oauth {
            return Ok(());
        }
        if !exclusive_live_needs_apply(adapter, agent, &current) {
            return Ok(());
        }
        tracing::info!(
            module = targets::ACCOUNT,
            op = "exclusive_live",
            agent = agent.as_str(),
            "current login is not the only live source; writing it back"
        );
        adapter.apply_account(&current.to_live())
    }

    /// Read-only live authentication status, including adapter-projection presence.
    pub fn probe_live_auth(&self, agent: AgentId) -> Result<AuthState> {
        let adapter = self.registry.get(agent).ok_or_else(|| {
            AppError::NotFound(format!("adapter not registered: {}", agent.as_str()))
        })?;
        let mut state = adapter.read_auth()?;
        if self.live_is_adapter_projection(agent).unwrap_or(false)
            && !state
                .also_present
                .iter()
                .any(|kind| kind == crate::services::ADAPTER_PROJECTION_KIND)
        {
            state
                .also_present
                .push(crate::services::ADAPTER_PROJECTION_KIND.to_owned());
        }
        Ok(state)
    }

    pub fn live_is_adapter_projection(&self, agent: AgentId) -> Result<bool> {
        let (profiles, providers, leftover) = self.projection_snapshots(agent)?;
        let Ok(adapter) = self.adapter(agent) else {
            return Ok(false);
        };
        let lives = match self.read_live_accounts(adapter.as_ref(), agent) {
            Ok(lives) => lives,
            Err(_) => {
                return Ok(providers.iter().any(|provider| {
                    provider.agent_id == agent
                        && provider.is_current
                        && crate::services::adapter_projection::generated_provider_is_adapter_owned(
                            provider,
                        )
                }));
            }
        };
        if lives.is_empty() {
            return Ok(false);
        }
        let origins: Vec<_> = lives
            .iter()
            .map(|live| {
                classify_account_live(
                    agent,
                    live.kind,
                    &live.credentials,
                    &profiles,
                    &providers,
                    leftover,
                )
            })
            .collect();
        if origins
            .iter()
            .any(|origin| matches!(origin, LiveOrigin::UserGrant))
        {
            return Ok(false);
        }
        Ok(origins.iter().all(|origin| origin.is_projection()))
    }

    pub(super) fn classify_live_account(
        &self,
        agent: AgentId,
        live: &LiveAccount,
    ) -> Result<LiveOrigin> {
        let (profiles, providers, leftover) = self.projection_snapshots(agent)?;
        Ok(classify_account_live(
            agent,
            live.kind,
            &live.credentials,
            &profiles,
            &providers,
            leftover,
        ))
    }

    pub(super) fn skip_projection_reconcile(&self, agent: AgentId, live: &LiveAccount) -> bool {
        let Ok((profiles, providers, leftover)) = self.projection_snapshots(agent) else {
            return leftover_live_flag(agent);
        };
        should_skip_live_reconcile(
            agent,
            live.kind,
            &live.credentials,
            &profiles,
            &providers,
            leftover,
        )
    }

    fn projection_snapshots(
        &self,
        agent: AgentId,
    ) -> Result<(Vec<AdapterProfile>, Vec<Provider>, bool)> {
        let profiles =
            AdapterProfileRepo::new(self.db.clone()).list_filtered(&AdapterProfileFilter {
                target_agent_id: Some(agent),
                ..Default::default()
            })?;
        let providers = ProviderRepo::new(self.db.clone()).list(Some(agent))?;
        Ok((profiles, providers, leftover_live_flag(agent)))
    }

    pub(super) fn validate_live_switch_identity(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        live: &LiveAccount,
    ) -> Result<()> {
        if leftover_shaped_codex_live(agent) {
            return Ok(());
        }
        let rows = self.repo.list(Some(agent))?;
        if !authorization_duplicates(adapter, agent, live.kind, &live.credentials, &rows).is_empty()
        {
            return Ok(());
        }
        // CodexAdapter::read_account always sets kind=Oauth even when the live
        // file is only OPENAI_API_KEY (no email). The 切换 click path hits this
        // validator, not a kind==ApiKey branch.
        let live_anonymous = stable_live_identity(adapter, live.kind, &live.credentials).is_none();
        let official_identity_known = rows.iter().any(stored_has_known_email);
        if live_anonymous {
            if live.kind == crate::models::AccountKind::ApiKey
                || live_is_api_key_shaped(live)
                || official_identity_known
            {
                return Ok(());
            }
            return Err(AppError::message(
                "account.identity_conflict",
                "live account identity is unknown; refusing to backfill or switch",
            ));
        }
        if rows.iter().any(|row| {
            row.is_current && stable_live_identity(adapter, row.kind, &row.credentials).is_none()
        }) {
            if official_identity_known || live_is_api_key_shaped(live) {
                return Ok(());
            }
            return Err(AppError::message(
                "account.identity_conflict",
                "current account identity is unknown; refusing to backfill or switch",
            ));
        }
        Ok(())
    }

    /// Force-refresh upstream 5h/7d quota windows for one OAuth account.
    pub(super) fn sync_current_account_live(
        &self,
        stored: &Account,
        api_key: Option<&str>,
        note: &str,
    ) -> Result<()> {
        let key_changed = api_key
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        if !stored.is_current || !key_changed {
            self.snapshot_after_pool_change(stored.agent_id, note);
            return Ok(());
        }
        let Some(backup) = self.backup.as_ref() else {
            return Ok(());
        };
        let adapter = match self.adapter(stored.agent_id) {
            Ok(adapter) => adapter,
            Err(_) => return Ok(()),
        };
        if !adapter.capability(Capability::AccountSwitch).is_usable() {
            self.snapshot_after_pool_change(stored.agent_id, note);
            return Ok(());
        }

        let live_before = match adapter.read_account() {
            Ok(live) => Some(live),
            Err(error) if error.code() == "not_found" => None,
            Err(error) => return Err(error),
        };
        let apply_live = stored.to_live();
        if live_before
            .as_ref()
            .is_some_and(|before| before.credentials == apply_live.credentials)
        {
            self.snapshot_after_pool_change(stored.agent_id, note);
            return Ok(());
        }

        let live_guard = backup.acquire_live_write(stored.agent_id)?;
        if let Err(error) = backup.snapshot_with_guard(
            &live_guard,
            stored.agent_id,
            BackupKind::AutoSwitch,
            Some(&format!("before applying current account {}", stored.id)),
        ) {
            if error.code() != "not_found" {
                return Err(error);
            }
        }
        if let Err(error) = adapter.apply_account(&apply_live) {
            // Codex/Pi (and similar) can store API-key accounts but refuse to
            // apply that format to live files. Keep the pool update; do not
            // fail the save or attempt a live rollback of an unapplied write.
            if error.code() == "unsupported" {
                let _ = backup.snapshot_with_guard(
                    &live_guard,
                    stored.agent_id,
                    BackupKind::AutoSwitch,
                    Some(note),
                );
                return Ok(());
            }
            let live_rollback = match &live_before {
                Some(before) => adapter.apply_account(before).err(),
                None => None,
            };
            return Err(compensated_current_account_apply_error(
                error,
                live_rollback,
            ));
        }
        Ok(())
    }
}

fn live_grant_matches_account(
    adapter: &dyn AgentAdapter,
    live: &LiveAccount,
    account: &Account,
) -> bool {
    accounts_same_authorization(adapter, live.kind, &live.credentials, account)
        || accounts_same_oauth_identity(live.kind, &live.credentials, account)
}

/// Live auth.json that is only an API key (Codex still reports Oauth).
fn live_is_api_key_shaped(live: &LiveAccount) -> bool {
    let body = live
        .credentials
        .get("body")
        .filter(|value| value.is_object())
        .unwrap_or(&live.credentials);
    let has_key = body
        .get("OPENAI_API_KEY")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .is_some_and(|key| !key.is_empty());
    let tokens = body.get("tokens");
    let has_oauth = tokens.is_some_and(|tokens| {
        ["access_token", "refresh_token"].iter().any(|field| {
            tokens
                .get(*field)
                .and_then(|value| value.as_str())
                .map(str::trim)
                .is_some_and(|token| !token.is_empty())
        })
    });
    has_key && !has_oauth
}

fn stored_has_known_email(row: &Account) -> bool {
    row.extra
        .get("email")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .is_some_and(|email| !email.is_empty())
        || find_stable_identity_field(&row.credentials)
            .is_some_and(|identity| identity.contains('@'))
}

fn leftover_shaped_codex_live(agent: AgentId) -> bool {
    leftover_live_flag(agent)
        || (agent == AgentId::Codex
            && crate::integrations::agents::codex::leftover::live_config_is_bridge_leftover())
}

fn exclusive_live_needs_apply(
    adapter: &dyn AgentAdapter,
    agent: AgentId,
    current: &Account,
) -> bool {
    match agent {
        AgentId::Codex => adapter.live_backup_paths().iter().any(|path| {
            path.file_name().is_some_and(|name| name == "config.toml")
                && std::fs::read_to_string(path).ok().is_some_and(|text| {
                    crate::integrations::agents::codex::leftover::toml_has_competing_api_key_pointer(
                        &text,
                    ) || crate::integrations::agents::codex::leftover::toml_is_bridge_leftover(&text)
                })
        }),
        AgentId::Pi => {
            adapter
                .live_backup_paths()
                .iter()
                .any(|path| path.file_name().is_some_and(|name| name == "settings.json"))
                && crate::adapters::pi::pi_oauth_live_slot_mismatch(current)
        }
        AgentId::Grok => adapter.live_backup_paths().iter().any(|path| {
            path.file_name().is_some_and(|name| name == "config.toml")
                && crate::adapters::grok_live_has_leftover_api_key_field()
        }),
        AgentId::Kimi => {
            adapter
                .live_backup_paths()
                .iter()
                .any(|path| path.file_name().is_some_and(|name| name == "config.toml"))
                && crate::adapters::kimi_live_has_leftover_api_key_when_oauth(current)
        }
        _ => false,
    }
}

pub(super) fn compensated_current_account_apply_error_with_db(
    primary: AppError,
    live_rollback: Option<AppError>,
    db_rollback: Option<AppError>,
) -> AppError {
    if live_rollback.is_none() && db_rollback.is_none() {
        return primary;
    }
    let live = live_rollback.as_ref().map_or("ok", AppError::code);
    let database = db_rollback.as_ref().map_or("ok", AppError::code);
    AppError::message(
        "account.current.apply.rollback",
        format!(
            "applying the current account failed [{}]; compensation status: live={live}, database={database}",
            primary.code()
        ),
    )
}
