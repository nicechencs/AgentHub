use std::time::Instant;

use uuid::Uuid;

use crate::error::Result;
use crate::models::{authorization_is_route_pool_home, Account, AccountInput};

use super::super::surface::*;
use super::super::{AccountService, MAX_ACCOUNT_LABEL_LEN};
use super::types::AccountMutationError;

impl AccountService {
    /// Create a pool account from a fully formed input (e.g. OAuth PKCE result).
    /// Does not write live credentials.
    pub fn create(&self, input: AccountInput) -> Result<Account> {
        let started = Instant::now();
        let agent = input.agent_id;
        let result = self.create_inner(input);
        log_account_op("create", agent, started, &result);
        result
    }

    pub(in crate::services::account_service) fn create_inner(
        &self,
        input: AccountInput,
    ) -> Result<Account> {
        validate_label(&input.label, "account label", MAX_ACCOUNT_LABEL_LEN)?;
        let label = input.label.trim().to_string();
        let adapter = self.adapter(input.agent_id).ok();
        let extra = if let Some(ref ad) = adapter {
            attach_identity_meta(
                ad.as_ref(),
                input.kind,
                &input.credentials,
                &label,
                input.extra,
            )
        } else {
            input.extra
        };

        let now = now_ts();
        let mut row = Account {
            id: format!("{}-acc-{}", input.agent_id.as_str(), Uuid::new_v4()),
            agent_id: input.agent_id,
            kind: input.kind,
            label,
            credentials: input.credentials,
            extra,
            status: "active".into(),
            is_current: input.is_current,
            created_at: now.clone(),
            updated_at: now,
        };
        row = self.prepare_account_surface(row);
        let _ = crate::services::account_identity_heal::heal_account_identity(&mut row);

        // A login started from Routes is intentionally a separate pool-owned
        // authorization. It must not be collapsed into an existing current
        // row when OAuth identity dedupe finds the same person. The marker is
        // stamped by the device-code flow before this service is called.
        if authorization_is_route_pool_home(&row.extra) {
            if row.is_current {
                return Err(crate::error::AppError::InvalidArg(
                    "route-pool-owned accounts cannot be current".into(),
                ));
            }
            if let Some(ref ad) = adapter {
                return self
                    .commit_pool_owned_authorization_merge(
                        ad.as_ref(),
                        &row,
                        input.kind,
                        row.label.clone(),
                        row.credentials.clone(),
                        row.extra.clone(),
                        false,
                    )
                    .map(|committed| committed.stored)
                    .map_err(AccountMutationError::into_error);
            }
            return self.repo.create(&row);
        }
        if let Some(ref ad) = adapter {
            return self
                .commit_authorization_merge(
                    ad.as_ref(),
                    &row,
                    input.kind,
                    row.label.clone(),
                    row.credentials.clone(),
                    row.extra.clone(),
                    input.is_current,
                )
                .map(|committed| committed.stored)
                .map_err(AccountMutationError::into_error);
        }
        if row.is_current {
            let (created, _binding) = self.connections.create_and_activate_account(&row)?;
            Ok(created)
        } else {
            let created = self.repo.create(&row)?;
            Ok(created)
        }
    }
}
