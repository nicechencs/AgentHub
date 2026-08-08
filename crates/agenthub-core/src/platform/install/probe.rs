//! Non-npm version probe descriptors (used by update_check_service).

/// Non-npm remote version feeds (official CDN / artifact hosts / install scripts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialVersionProbe {
    /// JSON body with top-level `"version"` (e.g. Kimi CDN `latest.json`).
    JsonVersion {
        url: &'static str,
        source: &'static str,
    },
    /// Plain-text version body (e.g. Grok GCS stable pointer).
    PlainVersion {
        url: &'static str,
        source: &'static str,
    },
    /// Official install script embeds the latest build id (Cursor Agent).
    ScriptVersion {
        url: &'static str,
        source: &'static str,
        kind: ScriptVersionKind,
    },
}

/// How to parse a [`OfficialVersionProbe::ScriptVersion`] body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptVersionKind {
    /// Cursor Agent official install script (`cursor.com/install…`).
    CursorInstall,
}

impl OfficialVersionProbe {
    pub fn url(self) -> &'static str {
        match self {
            Self::JsonVersion { url, .. }
            | Self::PlainVersion { url, .. }
            | Self::ScriptVersion { url, .. } => url,
        }
    }

    pub fn source(self) -> &'static str {
        match self {
            Self::JsonVersion { source, .. }
            | Self::PlainVersion { source, .. }
            | Self::ScriptVersion { source, .. } => source,
        }
    }

    pub fn cache_key(self) -> String {
        format!("feed|{}", self.url())
    }
}
