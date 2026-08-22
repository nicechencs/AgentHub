//! Account pool CRUD — query, API-key write, create, token refresh,
//! authorization merge, and post-commit compensation.
//!
//! Split for maintainability only. Public path stays [`crate::services::AccountService`].

mod api_key;
mod compensate;
mod create;
mod merge;
mod query;
mod refresh;
mod types;

#[allow(unused_imports)] // account_service glob (`use super::*`) keeps these names.
pub(super) use types::{AccountCommittedMutation, AccountMutationError};
