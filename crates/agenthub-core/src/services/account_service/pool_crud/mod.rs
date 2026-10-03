//! Account pool CRUD — query, API-key write, create, token refresh,
//! authorization merge, and post-commit compensation.
//!
//! Split for maintainability only. Public path stays [`crate::services::AccountService`].

mod api_key;
mod compensate;
mod create;
mod merge;
mod pi_live_delete;
mod query;
mod refresh;
mod types;

#[cfg(test)]
pub(super) use pi_live_delete::FAIL_PI_AUTH_REMOVE;
#[allow(unused_imports)] // account_service glob (`use super::*`) keeps these names.
pub(super) use types::{AccountCommittedMutation, AccountMutationError};
