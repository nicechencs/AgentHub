//! Opaque auth file revisions and credential-envelope inspection.

use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::utils::expiry::{is_expired, normalize_credential_key};

/// Metadata extracted from JSON credential envelopes without retaining any
/// credential values. It is intentionally limited to token presence and
/// expiry state so auth probes cannot leak secrets.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AuthCredentialMetadata {
    pub has_access_token: bool,
    pub has_refresh_token: bool,
    pub has_api_key: bool,
    pub access_expired: Option<bool>,
    pub refresh_expired: Option<bool>,
    pub has_identity: bool,
}

/// Return an opaque file revision derived from non-secret filesystem metadata.
///
/// Credential bytes are never read or hashed.  The canonical path is only an
/// input to the hash, so it is never exposed to clients.  In addition to the
/// full mtime precision and length, include platform file identity/change
/// metadata: a same-length atomic replacement can otherwise retain a coarse
/// timestamp and evade the optimistic live-switch check.
pub(crate) fn auth_file_revision(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    let normalized = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let fingerprint_input = format!(
        "auth-file-revision-v2\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
        normalized.to_string_lossy(),
        modified.as_secs(),
        modified.subsec_nanos(),
        metadata.len(),
        auth_file_identity(path, &metadata),
    );
    Some(format!("file:sha256:{}", sha256_hex(&fingerprint_input)))
}

/// Combine several opaque file revisions without exposing their paths or
/// metadata.  Callers retain the input order where that order is meaningful.
pub(crate) fn auth_files_revision(paths: &[&Path]) -> Option<String> {
    let revisions: Vec<String> = paths
        .iter()
        .filter_map(|path| auth_file_revision(path))
        .collect();
    (!revisions.is_empty()).then(|| {
        format!(
            "files:sha256:{}",
            sha256_hex(&format!(
                "auth-files-revision-v2\u{0}{}",
                revisions.join("\u{0}")
            ))
        )
    })
}

#[cfg(unix)]
fn auth_file_identity(_path: &Path, metadata: &std::fs::Metadata) -> String {
    use std::os::unix::fs::MetadataExt;

    format!(
        "unix:{}:{}:{}:{}",
        metadata.dev(),
        metadata.ino(),
        metadata.ctime(),
        metadata.ctime_nsec()
    )
}

#[cfg(windows)]
mod win_file_info {
    #[repr(C)]
    pub(super) struct FileTime {
        low_date_time: u32,
        high_date_time: u32,
    }

    #[repr(C)]
    // Mirrors the Win32 `BY_HANDLE_FILE_INFORMATION` layout exactly; adding
    // or reordering fields shifts the file-index offsets the API writes.
    pub(crate) struct ByHandleFileInformation {
        pub(super) file_attributes: u32,
        pub(super) creation_time: FileTime,
        pub(super) last_access_time: FileTime,
        pub(super) last_write_time: FileTime,
        pub(super) volume_serial_number: u32,
        pub(super) file_size_high: u32,
        pub(super) file_size_low: u32,
        pub(super) number_of_links: u32,
        pub(super) file_index_high: u32,
        pub(super) file_index_low: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub(super) fn GetFileInformationByHandle(
            file: *mut std::ffi::c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }
}

/// Volume serial number + 64-bit file index for `path`, or `None` when the
/// handle information cannot be queried. Shared with `backup_service` tests.
#[cfg(windows)]
pub(crate) fn windows_file_key(path: &Path) -> Option<(u32, u64)> {
    use std::os::windows::io::AsRawHandle;

    let file = std::fs::File::open(path).ok()?;
    let mut information = std::mem::MaybeUninit::<win_file_info::ByHandleFileInformation>::zeroed();
    // SAFETY: `file` owns a valid handle for the duration of this call and the
    // buffer is correctly sized for the documented Win32 structure.
    let ok = unsafe {
        win_file_info::GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr())
    };
    if ok == 0 {
        return None;
    }
    // SAFETY: the API initialized the buffer (non-zero return above).
    let information = unsafe { information.assume_init() };
    let file_index =
        (u64::from(information.file_index_high) << 32) | u64::from(information.file_index_low);
    Some((information.volume_serial_number, file_index))
}

#[cfg(windows)]
fn auth_file_identity(path: &Path, metadata: &std::fs::Metadata) -> String {
    use std::os::windows::fs::MetadataExt;

    let fallback = || {
        format!(
            "windows:fallback:{}:{}:{}",
            metadata.creation_time(),
            metadata.last_write_time(),
            metadata.len(),
        )
    };
    let Some((volume_serial_number, file_index)) = windows_file_key(path) else {
        return fallback();
    };
    format!(
        "windows:{}:{}:{}:{}",
        volume_serial_number,
        file_index,
        metadata.creation_time(),
        metadata.last_write_time(),
    )
}

#[cfg(not(any(unix, windows)))]
fn auth_file_identity(_path: &Path, metadata: &std::fs::Metadata) -> String {
    // Keep a metadata-only fallback for less common targets.  mtime precision
    // and length are already part of the enclosing fingerprint.
    format!(
        "fallback:{}:{}",
        metadata.len(),
        metadata.permissions().readonly()
    )
}

fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};

    Sha256::digest(input.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Inspect a credential JSON object recursively. Only key names and whether
/// a non-empty value exists are retained; token values are dropped immediately.
pub(crate) fn inspect_auth_credentials(value: &serde_json::Value) -> AuthCredentialMetadata {
    fn visit(value: &serde_json::Value, out: &mut AuthCredentialMetadata) {
        let Some(object) = value.as_object() else {
            return;
        };
        for (raw_key, value) in object {
            let key = normalize_credential_key(raw_key);
            let non_empty = value.as_str().map(str::trim).is_some_and(|s| !s.is_empty());
            match key.as_str() {
                "access" | "access_token" | "accesstoken" | "id_token" | "idtoken" => {
                    out.has_access_token |= non_empty;
                    if let Some(expired) = is_expired(value) {
                        out.access_expired = Some(expired);
                    }
                }
                "refresh" | "refresh_token" | "refreshtoken" => {
                    out.has_refresh_token |= non_empty;
                    if let Some(expired) = is_expired(value) {
                        out.refresh_expired = Some(expired);
                    }
                }
                "expires" | "expires_at" | "expiresat" => {
                    if let Some(expired) = is_expired(value) {
                        out.access_expired = Some(expired);
                    }
                }
                "refresh_expires" | "refresh_expires_at" | "refreshexpiresat" => {
                    if let Some(expired) = is_expired(value) {
                        out.refresh_expired = Some(expired);
                    }
                }
                "api_key" | "apikey" | "openai_api_key" | "key" => {
                    out.has_api_key |= non_empty;
                }
                "email" | "email_address" | "emailaddress" | "user_id" | "userid"
                | "account_id" | "accountid" | "sub" | "name" => {
                    out.has_identity |= non_empty;
                }
                _ => {}
            }
            visit(value, out);
        }
    }

    let mut out = AuthCredentialMetadata::default();
    visit(value, &mut out);
    out
}

/// Derive OAuth health from only explicit token and expiry evidence.
///
/// A refresh token is considered renewable unless its own expiry is explicitly
/// known to have passed. If the refresh token is explicitly expired, an access
/// token that is still valid (or whose expiry is unknown) remains configured;
/// it becomes `NeedsLogin` when the access token is also known expired or is
/// absent altogether.
pub(crate) fn oauth_auth_health(metadata: AuthCredentialMetadata) -> crate::models::AuthHealth {
    use crate::models::AuthHealth;

    match (
        metadata.has_access_token,
        metadata.access_expired,
        metadata.has_refresh_token,
        metadata.refresh_expired,
    ) {
        (false, _, _, Some(true)) => AuthHealth::NeedsLogin,
        (_, Some(true), true, Some(true)) | (_, Some(true), false, _) => AuthHealth::NeedsLogin,
        (_, _, true, Some(false) | None) => AuthHealth::Renewable,
        _ => AuthHealth::Configured,
    }
}
