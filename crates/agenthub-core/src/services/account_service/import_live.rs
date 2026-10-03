use std::time::Instant;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::adapters::AgentAdapter;
use crate::error::{AppError, Result};
use crate::models::{
    attach_persisted_surface, Account, AgentId, Capability, ImportLiveFailedLogin,
    ImportLiveReport, ImportLiveRestoredLogin, LiveAccount, PersistedTicketSurface, TicketSurface,
    TRASH_HOME_CONNECTIONS,
};
use crate::services::adapter_projection::projection_import_error;
use crate::services::AdapterRouteService;
use crate::storage::ConnectionTrashRepo;

use super::surface::*;
use super::{AccountService, MAX_ACCOUNT_LABEL_LEN};

/// Result of writing one local login into the pool.
pub(super) enum LiveUpsertOutcome {
    /// Stored. `restored` is set when the login came back from the login
    /// recycle bin instead of being created anew.
    Imported {
        account: Account,
        restored: Option<ImportLiveRestoredLogin>,
    },
    SkippedInTrash,
}

/// How a live upsert treats a matching recycle-bin login.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TrashMatch {
    /// Legacy rule (Kiro refresh): restore only when the row becomes current,
    /// otherwise leave it in the recycle bin and store nothing.
    SkipUnlessCurrent,
    /// 「导入本机登录」: a login the user imports again always comes back.
    /// A match in the login recycle bin is restored; a match that only sits
    /// in the connection pool recycle bin stays there and the import creates
    /// a new connection.
    RestoreOnImport,
}

impl AccountService {
    /// Import the local login and return only the focused row. Kept for CLI
    /// and older callers; the desktop uses [`Self::import_live_report`].
    pub fn import_live(&self, agent: AgentId, name: Option<&str>) -> Result<Account> {
        self.import_live_report(agent, name)?
            .account
            .ok_or_else(|| AppError::message("account.import", "live import produced no accounts"))
    }

    /// User-triggered import with what was restored, skipped or failed, so the
    /// UI can explain the result.
    pub fn import_live_report(
        &self,
        agent: AgentId,
        name: Option<&str>,
    ) -> Result<ImportLiveReport> {
        let started = Instant::now();
        let result = self.import_live_inner(agent, name);
        if result.is_ok() {
            self.snapshot_after_pool_change(agent, "after live account import");
        }
        log_account_op("import", agent, started, &result);
        result
    }

    pub(super) fn import_live_inner(
        &self,
        agent: AgentId,
        name: Option<&str>,
    ) -> Result<ImportLiveReport> {
        // Pi stores multi-provider credentials in one auth.json — expand to
        // one pool row per provider so Connections can show each OAuth login.
        if agent == AgentId::Pi {
            return self.import_pi_providers_inner(name);
        }

        // Kiro / Cursor can import a local login without writing it back.
        let adapter = if matches!(agent, AgentId::Kiro | AgentId::Cursor) {
            self.registry.get(agent).ok_or_else(|| {
                AppError::NotFound(format!("adapter not registered: {}", agent.as_str()))
            })?
        } else {
            self.registry.require(agent, Capability::AccountSwitch)?
        };
        let _lock = self.acquire_live_lock(agent)?;
        let lives = self.read_live_accounts(adapter.as_ref(), agent)?;
        if lives.is_empty() {
            return Err(AppError::NotFound(
                "no live account credentials found".into(),
            ));
        }

        // 「同步当前登录」is a user override: always copy the live file onto the
        // matching row. Do not run rt/mtime bidirectional overlay here.
        // Grok nested auth.json slots import one row per person, like Pi
        // providers. Keep an existing current if that person is still in the
        // file; otherwise activate the default `::client` slot, not last-sorted.
        let mut report = ImportLiveReport::default();
        let mut grants = Vec::new();
        for live in lives {
            if self.classify_live_account(agent, &live)?.is_projection() {
                report.skipped_local_route += 1;
                continue;
            }
            grants.push(live);
        }
        if grants.is_empty() {
            if report.skipped_local_route > 0 {
                return Err(projection_import_error());
            }
            return Err(AppError::message(
                "account.import",
                "live import produced no accounts",
            ));
        }
        let current = self.repo.get_current(agent)?;
        let chosen =
            self.pick_live_grant_to_activate(adapter.as_ref(), agent, &grants, current.as_ref());
        let mut chosen_live = None;
        let mut others = Vec::new();
        for (index, live) in grants.into_iter().enumerate() {
            if index == chosen {
                chosen_live = Some(live);
            } else {
                others.push(live);
            }
        }
        for live in others {
            let outcome = self.upsert_live_account_outcome(
                adapter.as_ref(),
                agent,
                live,
                None,
                false,
                TrashMatch::RestoreOnImport,
            )?;
            record_import_outcome(&mut report, outcome, false);
        }
        let live = chosen_live.ok_or_else(|| {
            AppError::message("account.import", "live import produced no accounts")
        })?;
        let outcome = self.upsert_live_account_outcome(
            adapter.as_ref(),
            agent,
            live,
            name,
            true,
            TrashMatch::RestoreOnImport,
        )?;
        record_import_outcome(&mut report, outcome, true);
        if report.account.is_none() {
            return Err(AppError::message(
                "account.import",
                "live import produced no accounts",
            ));
        }
        Ok(report)
    }

    /// Import each Pi auth.json provider as its own pool account.
    /// `account` is the last imported row for UI focus. Pi providers are
    /// concurrent entries in one live file, so import does not guess a global
    /// current provider. One failing provider does not hide the ones that
    /// already imported: failures are collected and reported together.
    pub(super) fn import_pi_providers_inner(&self, name: Option<&str>) -> Result<ImportLiveReport> {
        let adapter = self
            .registry
            .require(AgentId::Pi, Capability::AccountSwitch)?;
        let _lock = self.acquire_live_lock(AgentId::Pi)?;
        let body = crate::adapters::pi_auth::read_auth_json()?;
        let lives = crate::adapters::pi_auth::expand_auth_to_live_accounts(&body)?;
        if lives.is_empty() {
            return Err(AppError::NotFound(
                "Pi auth.json has no provider credentials to import".into(),
            ));
        }
        self.import_pi_live_entries(adapter.as_ref(), lives, name)
    }

    /// Body of the Pi import once auth.json is expanded. Split out so the
    /// per-entry reporting can be tested without the global Pi config dir.
    /// Caller holds the Pi live lock.
    pub(super) fn import_pi_live_entries(
        &self,
        adapter: &dyn AgentAdapter,
        lives: Vec<LiveAccount>,
        name: Option<&str>,
    ) -> Result<ImportLiveReport> {
        let mut report = ImportLiveReport::default();
        let mut grants = Vec::new();
        for live in lives {
            if self
                .classify_live_account(AgentId::Pi, &live)?
                .is_projection()
            {
                report.skipped_local_route += 1;
                continue;
            }
            grants.push(live);
        }
        if grants.is_empty() {
            if report.skipped_local_route > 0 {
                return Err(projection_import_error());
            }
            return Err(AppError::message(
                "account.import",
                "Pi import produced no accounts",
            ));
        }
        let n = grants.len();
        let mut errors = Vec::new();
        for (i, live) in grants.into_iter().enumerate() {
            let display_name = if i + 1 == n { name } else { None };
            let entry_label = live
                .label_hint
                .as_deref()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("Pi #{}", i + 1));
            match self.upsert_live_account_outcome(
                adapter,
                AgentId::Pi,
                live,
                display_name,
                false,
                TrashMatch::RestoreOnImport,
            ) {
                Ok(outcome) => record_import_outcome(&mut report, outcome, true),
                Err(error) => {
                    tracing::warn!(
                        module = crate::logging::targets::ACCOUNT,
                        agent = "pi",
                        code = %error.code(),
                        "Pi import entry failed"
                    );
                    report.failed.push(ImportLiveFailedLogin {
                        label: entry_label,
                        code: error.code().to_string(),
                        message: error.to_string(),
                    });
                    errors.push(error);
                }
            }
        }
        if report.account.is_some() {
            return Ok(report);
        }
        if errors.len() == 1 {
            return Err(errors.remove(0));
        }
        if !report.failed.is_empty() {
            let details = report
                .failed
                .iter()
                .map(|failed| format!("{}：{}", failed.label, failed.message))
                .collect::<Vec<_>>()
                .join("；");
            return Err(AppError::message(
                "account.import",
                format!("{} 个登录都没导入成功。{details}", report.failed.len()),
            ));
        }
        Err(AppError::message(
            "account.import",
            "Pi import produced no accounts",
        ))
    }

    /// Kept for callers that only need the stored row (Kiro refresh, tests).
    /// A login whose match sits in the recycle bin yields `None` unless it
    /// becomes current.
    pub(super) fn upsert_live_account(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        live: LiveAccount,
        name: Option<&str>,
        make_current: bool,
    ) -> Result<Option<Account>> {
        Ok(
            match self.upsert_live_account_outcome(
                adapter,
                agent,
                live,
                name,
                make_current,
                TrashMatch::SkipUnlessCurrent,
            )? {
                LiveUpsertOutcome::Imported { account, .. } => Some(account),
                LiveUpsertOutcome::SkippedInTrash => None,
            },
        )
    }

    pub(super) fn upsert_live_account_outcome(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        live: LiveAccount,
        name: Option<&str>,
        make_current: bool,
        trash_match: TrashMatch,
    ) -> Result<LiveUpsertOutcome> {
        if live.agent != agent {
            return Err(AppError::InvalidArg(format!(
                "adapter returned account for {}, expected {}",
                live.agent.as_str(),
                agent.as_str()
            )));
        }

        let trash_home = match trash_match {
            TrashMatch::SkipUnlessCurrent => None,
            TrashMatch::RestoreOnImport => Some(TRASH_HOME_CONNECTIONS),
        };
        let mut restored = None;
        if let Some(entry) = self.matching_live_trash_entry(adapter, agent, &live, trash_home)? {
            if trash_match == TrashMatch::SkipUnlessCurrent && !make_current {
                tracing::debug!(
                    module = crate::logging::targets::ACCOUNT,
                    agent = agent.as_str(),
                    "live upsert skipped a recycle-bin login"
                );
                return Ok(LiveUpsertOutcome::SkippedInTrash);
            }
            let (trash_id, label) = entry;
            self.connections.restore_trash(&trash_id)?;
            restored = Some(label);
        }

        let display = name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or(live.label_hint.clone())
            .unwrap_or_else(|| format!("Imported {}", now_ts()));
        validate_label(&display, "account label", MAX_ACCOUNT_LABEL_LEN)?;

        let mut extra = live.extra;
        if let Some(obj) = extra.as_object_mut() {
            obj.insert("source".into(), json!("live"));
        }
        let extra = attach_identity_meta(adapter, live.kind, &live.credentials, &display, extra);

        let now = now_ts();
        let mut row = Account {
            id: format!("{}-live-{}", agent.as_str(), Uuid::new_v4()),
            agent_id: agent,
            kind: live.kind,
            label: display,
            credentials: live.credentials,
            extra,
            status: "active".into(),
            is_current: make_current,
            created_at: now.clone(),
            updated_at: now,
        };
        row = self.prepare_account_surface(row);
        let _ = crate::services::account_identity_heal::heal_account_identity(&mut row);
        self.commit_authorization_merge(
            adapter,
            &row,
            live.kind,
            row.label.clone(),
            row.credentials.clone(),
            row.extra.clone(),
            make_current,
        )
        .map(|committed| LiveUpsertOutcome::Imported {
            restored: restored.map(|label| ImportLiveRestoredLogin {
                id: committed.stored.id.clone(),
                label,
            }),
            account: committed.stored,
        })
        .map_err(|error| error.into_error())
    }

    /// Recycle-bin login with the same authorization (and, for Pi, the same
    /// provider): `(recycle-bin id, label)`, so an import can say what it
    /// restored. `home` limits the search to one recycle bin.
    fn matching_live_trash_entry(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        live: &LiveAccount,
        home: Option<&str>,
    ) -> Result<Option<(String, String)>> {
        let now = chrono::Utc::now()
            .format("%Y-%m-%d %H:%M:%S%.6f")
            .to_string();
        let items = ConnectionTrashRepo::new(self.db.clone()).list(Some(agent), home, &now)?;
        Ok(items.iter().find_map(|item| {
            let account = item.account.as_ref()?;
            (same_live_slot(agent, &live.credentials, &account.credentials)
                && accounts_same_authorization(adapter, live.kind, &live.credentials, account))
            .then(|| {
                let label = Some(item.label.trim())
                    .filter(|label| !label.is_empty())
                    .unwrap_or(account.label.as_str())
                    .to_string();
                (item.id.clone(), label)
            })
        }))
    }

    /// Add the ticket surface to a prospective row before its first database
    /// mutation. Only a missing `extra.surface` is filled; Unrecognized and
    /// Known values are left untouched so a newer/future surface cannot be
    /// overwritten by this version's classifier.
    pub(super) fn prepare_account_surface(&self, mut account: Account) -> Account {
        if TicketSurface::from_persisted_json(&account.extra) != PersistedTicketSurface::Missing {
            return account;
        }
        let product = AdapterRouteService::classify_account_source_product(&account);
        attach_persisted_surface(&mut account.extra, TicketSurface::from_product(product));
        account
    }

    pub(super) fn copy_persisted_surface(from: &Value, into: &mut Value) {
        let Some(surface) = from.get("surface") else {
            return;
        };
        if let Some(obj) = into.as_object_mut() {
            obj.insert("surface".into(), surface.clone());
        }
    }

    /// Keep last4 / host / hash already stamped on the pool row when a live
    /// snapshot arrives without them (redacted `***`, leftover import). Never
    /// invents a key or copies another login's identity.
    pub(super) fn copy_persisted_identity(from: &Value, into: &mut Value) {
        for key in [
            "secretTail",
            "secretHash",
            "endpoint",
            "baseUrl",
            "base_url",
        ] {
            copy_missing_extra_string(from, into, key);
        }
        let old_label = from
            .get("identityLabel")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        let new_label = into
            .get("identityLabel")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        let old_has_identity = crate::utils::redact::secret_tail_from_masked_preview(old_label)
            .is_some()
            || (old_label.contains('@') && !old_label.contains(' '));
        let new_is_weak = new_label.is_empty() || new_label.eq_ignore_ascii_case("API Key");
        if old_has_identity && new_is_weak {
            if let Some(obj) = into.as_object_mut() {
                obj.insert("identityLabel".into(), json!(old_label));
            }
        }
    }

    /// Repair a legacy row's surface using a narrow optimistic update. Only
    /// `extra.surface` and `updated_at` are written; credentials, label,
    /// current state and active binding are never copied from a stale caller.
    pub(super) fn stamp_account_surface(&self, account: Account) -> Result<Account> {
        let prepared = self.prepare_account_surface(account.clone());
        if prepared.extra == account.extra {
            return Ok(account);
        }
        let expected_updated_at = account.updated_at.clone();
        let updated_at = now_ts();
        let extra = serde_json::to_string(&prepared.extra)?;
        let changed = self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE accounts SET extra = ?2, updated_at = ?3 WHERE id = ?1 AND agent_id = ?4 AND updated_at = ?5",
                rusqlite::params![
                    &account.id,
                    extra,
                    &updated_at,
                    account.agent_id.as_str(),
                    &expected_updated_at,
                ],
            )
            .map_err(AppError::from)
        })?;
        if changed != 1 {
            return Err(AppError::message(
                "account.conflict",
                format!("account changed before surface update: {}", account.id),
            ));
        }
        self.repo
            .get_by_id(&account.id)?
            .ok_or_else(|| AppError::NotFound(format!("account not found: {}", account.id)))
    }
}

fn copy_missing_extra_string(from: &Value, into: &mut Value, key: &str) {
    let incoming = into
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if !incoming.is_empty() {
        return;
    }
    let Some(value) = from
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    if let Some(obj) = into.as_object_mut() {
        obj.insert(key.into(), json!(value));
    }
}

/// Fold one upsert result into the import report. `focus` marks the row the
/// UI should select afterwards.
fn record_import_outcome(report: &mut ImportLiveReport, outcome: LiveUpsertOutcome, focus: bool) {
    if let LiveUpsertOutcome::Imported { account, restored } = outcome {
        report.imported_count += 1;
        if let Some(restored) = restored {
            report.restored_from_trash.push(restored);
        }
        if focus {
            report.account = Some(account);
        }
    }
}
