//! PKCE S256 helpers.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

use crate::error::{AppError, Result};

#[derive(Debug, Clone)]
pub struct PkcePair {
    verifier: String,
    challenge: String,
}

impl PkcePair {
    pub fn generate() -> Result<Self> {
        let mut raw = [0u8; 32];
        getrandom::getrandom(&mut raw)
            .map_err(|e| AppError::message("oauth.pkce", format!("random failed: {e}")))?;
        let verifier = URL_SAFE_NO_PAD.encode(raw);
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());
        Ok(Self {
            verifier,
            challenge,
        })
    }

    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    pub fn challenge(&self) -> &str {
        &self.challenge
    }
}

pub fn random_state() -> Result<String> {
    let mut raw = [0u8; 16];
    getrandom::getrandom(&mut raw)
        .map_err(|e| AppError::message("oauth.state", format!("random failed: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_shapes() {
        let p = PkcePair::generate().unwrap();
        assert!(p.verifier().len() >= 32);
        assert!(!p.challenge().is_empty());
        assert_ne!(p.verifier(), p.challenge());
    }
}
