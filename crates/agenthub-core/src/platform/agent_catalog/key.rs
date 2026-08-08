//! Open agent identifier (`AgentKey`) — stable kebab-case string newtype.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::models::AgentId;

/// Format / parse failure for [`AgentKey`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentKeyError {
    pub value: String,
    pub reason: &'static str,
}

impl fmt::Display for AgentKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid agent key '{}': {}", self.value, self.reason)
    }
}

impl std::error::Error for AgentKeyError {}

impl From<AgentKeyError> for AppError {
    fn from(err: AgentKeyError) -> Self {
        AppError::InvalidArg(err.to_string())
    }
}

/// Stable agent identifier for catalog / DB / cross-end contracts.
///
/// Target format: lowercase kebab-case (`claude`, `codex`, `claude-code`).
/// Existing [`AgentId::as_str`] values are a lossless subset of this format.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentKey(String);

impl AgentKey {
    /// Parse and validate a key. Rejects empty, uppercase, underscores, and
    /// malformed hyphen placement.
    pub fn parse(value: impl Into<String>) -> std::result::Result<Self, AgentKeyError> {
        let value = value.into();
        validate_key(&value)?;
        Ok(Self(value))
    }

    /// Infallible conversion from the closed [`AgentId`] enum.
    ///
    /// `AgentId::as_str()` is always a valid kebab-case key.
    pub fn from_agent_id(id: AgentId) -> Self {
        // Lossless: enum string ids are compile-time fixed and already validated.
        Self(id.as_str().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for AgentKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for AgentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AgentKey {
    type Err = AgentKeyError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl From<AgentId> for AgentKey {
    fn from(id: AgentId) -> Self {
        Self::from_agent_id(id)
    }
}

impl TryFrom<String> for AgentKey {
    type Error = AgentKeyError;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for AgentKey {
    type Error = AgentKeyError;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::parse(value)
    }
}

/// Parse helper that maps format errors to [`AppError::InvalidArg`].
pub fn parse_agent_key(value: &str) -> Result<AgentKey> {
    AgentKey::parse(value).map_err(AppError::from)
}

fn validate_key(value: &str) -> std::result::Result<(), AgentKeyError> {
    if value.is_empty() {
        return Err(AgentKeyError {
            value: value.to_string(),
            reason: "must not be empty",
        });
    }
    // Lowercase kebab-case: one or more segments of [a-z0-9]+ joined by single '-'.
    // First character must be a-z (not a digit-only key).
    let mut chars = value.chars().peekable();
    let first = *chars.peek().expect("non-empty");
    if !first.is_ascii_lowercase() {
        return Err(AgentKeyError {
            value: value.to_string(),
            reason: "must start with a lowercase letter (kebab-case)",
        });
    }

    let mut prev_hyphen = false;
    for c in value.chars() {
        match c {
            'a'..='z' | '0'..='9' => prev_hyphen = false,
            '-' => {
                if prev_hyphen {
                    return Err(AgentKeyError {
                        value: value.to_string(),
                        reason: "must not contain consecutive hyphens",
                    });
                }
                prev_hyphen = true;
            }
            _ => {
                return Err(AgentKeyError {
                    value: value.to_string(),
                    reason: "only lowercase letters, digits, and single hyphens allowed",
                });
            }
        }
    }
    if prev_hyphen {
        return Err(AgentKeyError {
            value: value.to_string(),
            reason: "must not end with a hyphen",
        });
    }
    Ok(())
}
