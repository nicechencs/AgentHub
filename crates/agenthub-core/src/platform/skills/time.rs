//! Shared timestamps for skill package / assignment writes.

pub(crate) fn chrono_now() -> String {
    // Sub-second precision so install→update in the same wall-clock second still
    // yields a distinct package revision (revision falls back to updated_at).
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
