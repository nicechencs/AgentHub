//! Read snapshot files for the backup details pane.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{AppError, Result};
use crate::models::{
    AgentId, BackupFact, BackupFileView, BackupInspect, BackupListItem, BackupRecord,
};
use crate::utils::redact::{mask_secret_tail, refresh_token_tail};

use super::path_safety::{classify_path, is_path_inside, PathClass};
use super::snapshot::{read_manifest, ManifestEntry};
use super::BackupService;

const MAX_PREVIEW_BYTES: u64 = 256 * 1024;

#[derive(Default)]
struct FactSet {
    email: Option<String>,
    secret_tail: Option<String>,
    endpoint: Option<String>,
    provider: Option<String>,
    model: Option<String>,
}

impl BackupService {
    /// List backups with a short identity for GUI cards.
    pub fn list_with_identity(&self, agent: Option<AgentId>) -> Result<Vec<BackupListItem>> {
        let rows = self.list(agent)?;
        Ok(rows
            .into_iter()
            .map(|record| {
                let identity = self
                    .inspect_record(&record, false)
                    .ok()
                    .and_then(|inspect| inspect.identity);
                BackupListItem { record, identity }
            })
            .collect())
    }

    /// File contents + distinguishing facts for one backup.
    pub fn inspect(&self, id: &str) -> Result<BackupInspect> {
        let record = self.get_by_id(id)?;
        self.inspect_record(&record, true)
    }

    fn inspect_record(
        &self,
        record: &BackupRecord,
        include_content: bool,
    ) -> Result<BackupInspect> {
        let dir = self.validate_snapshot_dir(record)?;
        let manifest = read_manifest(&dir).ok().flatten();
        let entries: Vec<(String, Option<String>)> = if let Some(manifest) = manifest {
            manifest
                .entries
                .into_iter()
                .map(|ManifestEntry { stored, source, .. }| (stored, Some(source)))
                .collect()
        } else {
            record
                .files
                .iter()
                .cloned()
                .map(|name| (name, None))
                .collect()
        };

        let mut files = Vec::new();
        for (name, source) in entries {
            match preview_stored_file(&dir, &name, source.as_deref(), include_content) {
                Ok(file) => files.push(file),
                Err(_) => files.push(BackupFileView {
                    name: name.clone(),
                    source: source.map(display_path_str),
                    path: dir.join(&name).display().to_string(),
                    size: 0,
                    content: None,
                    facts: Vec::new(),
                }),
            }
        }

        let mut facts = FactSet::default();
        for file in &files {
            merge_facts(&mut facts, &file.facts);
        }
        let identity = facts.identity();
        Ok(BackupInspect {
            id: record.id.clone(),
            agent_id: record.agent_id,
            kind: record.kind,
            created_at: record.created_at.clone(),
            size: record.size,
            note: record.note.clone(),
            identity,
            facts: facts.into_facts(),
            files,
        })
    }
}

fn preview_stored_file(
    dir: &Path,
    name: &str,
    source: Option<&str>,
    include_content: bool,
) -> Result<BackupFileView> {
    let path = dir.join(name);
    if !is_path_inside(&path, dir) {
        return Err(AppError::message(
            "backup.path",
            format!("snapshot file escapes backup dir: {}", path.display()),
        ));
    }
    match classify_path(&path)? {
        PathClass::RegularFile => {}
        PathClass::Missing => {
            return Ok(BackupFileView {
                name: name.into(),
                source: source.map(display_path_str),
                path: display_path(&path),
                size: 0,
                content: None,
                facts: Vec::new(),
            });
        }
        PathClass::Symlink | PathClass::Directory | PathClass::Other => {
            return Ok(BackupFileView {
                name: name.into(),
                source: source.map(display_path_str),
                path: display_path(&path),
                size: 0,
                content: None,
                facts: Vec::new(),
            });
        }
    }

    let size = std::fs::metadata(&path)?.len();
    let read_len = std::cmp::min(size, MAX_PREVIEW_BYTES) as usize;
    let mut bytes = vec![0_u8; read_len];
    let n = std::fs::File::open(&path)?.read(&mut bytes)?;
    bytes.truncate(n);
    let mut facts = FactSet::default();
    let content = match std::str::from_utf8(&bytes) {
        Ok(text) => {
            collect_from_text(text, &mut facts);
            if include_content {
                Some(file_preview(text))
            } else {
                None
            }
        }
        Err(_) => None,
    };

    Ok(BackupFileView {
        name: name.into(),
        source: source.map(display_path_str),
        path: display_path(&path),
        size,
        content,
        facts: facts.into_facts(),
    })
}

fn file_preview(text: &str) -> String {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(value) = serde_json::from_str::<Value>(text) {
            if let Ok(pretty) = serde_json::to_string_pretty(&value) {
                return pretty;
            }
        }
    }
    text.to_string()
}

fn collect_from_text(text: &str, facts: &mut FactSet) {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(value) = serde_json::from_str::<Value>(text) {
            collect_from_value(&value, facts);
            if facts.secret_tail.is_none() {
                if let Some(tail) = refresh_token_tail(&value) {
                    facts.secret_tail = Some(tail);
                }
            }
            return;
        }
    }
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((raw_key, raw_val)) = split_assign(line) else {
            continue;
        };
        consider_field(&raw_key.to_ascii_lowercase(), strip_quotes(raw_val), facts);
    }
}

fn split_assign(line: &str) -> Option<(&str, &str)> {
    if let Some(idx) = line.find('=') {
        return Some((line[..idx].trim(), line[idx + 1..].trim()));
    }
    if let Some(idx) = line.find(':') {
        return Some((line[..idx].trim(), line[idx + 1..].trim()));
    }
    None
}

fn strip_quotes(value: &str) -> &str {
    let t = value.trim().trim_end_matches(',');
    if t.len() >= 2 {
        let bytes = t.as_bytes();
        if (bytes[0] == b'"' && bytes[t.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[t.len() - 1] == b'\'')
        {
            return &t[1..t.len() - 1];
        }
    }
    t
}

fn collect_from_value(value: &Value, facts: &mut FactSet) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if let Some(text) = child.as_str() {
                    consider_field(&key.to_ascii_lowercase(), text, facts);
                }
                collect_from_value(child, facts);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_from_value(item, facts);
            }
        }
        _ => {}
    }
}

fn consider_field(key: &str, value: &str, facts: &mut FactSet) {
    let v = value.trim();
    if v.is_empty() || v == "***" {
        return;
    }
    if looks_like_email(v) && facts.email.is_none() {
        facts.email = Some(v.to_string());
    }
    match key {
        "email" | "email_address" | "emailaddress" => {
            if looks_like_email(v) {
                facts.email = Some(v.to_string());
            }
        }
        "api_key"
        | "apikey"
        | "access_token"
        | "accesstoken"
        | "refresh_token"
        | "refreshtoken"
        | "auth_token"
        | "authtoken"
        | "anthropic_auth_token" => {
            if facts.secret_tail.is_none() {
                facts.secret_tail = mask_secret_tail(v);
            }
        }
        "base_url" | "baseurl" | "endpoint" | "anthropic_base_url" => {
            if facts.endpoint.is_none() {
                facts.endpoint = Some(host_of(v).unwrap_or_else(|| v.to_string()));
            }
        }
        "provider" => {
            if facts.provider.is_none() {
                facts.provider = Some(v.to_string());
            }
        }
        "model" | "default_model" | "defaultmodel" => {
            if facts.model.is_none() && !v.starts_with("http") {
                facts.model = Some(v.to_string());
            }
        }
        _ => {}
    }
}

fn looks_like_email(value: &str) -> bool {
    let t = value.trim();
    t.contains('@') && !t.contains(' ') && t.len() < 254 && !t.starts_with('@')
}

fn host_of(url: &str) -> Option<String> {
    let rest = url
        .trim()
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest).trim();
    (!host.is_empty()).then(|| host.to_string())
}

fn merge_facts(into: &mut FactSet, facts: &[BackupFact]) {
    for fact in facts {
        match fact.key.as_str() {
            "email" if into.email.is_none() => into.email = Some(fact.value.clone()),
            "secretTail" if into.secret_tail.is_none() => {
                into.secret_tail = Some(fact.value.clone())
            }
            "endpoint" if into.endpoint.is_none() => into.endpoint = Some(fact.value.clone()),
            "provider" if into.provider.is_none() => into.provider = Some(fact.value.clone()),
            "model" if into.model.is_none() => into.model = Some(fact.value.clone()),
            _ => {}
        }
    }
}

impl FactSet {
    fn identity(&self) -> Option<String> {
        self.email
            .clone()
            .or_else(|| self.secret_tail.clone())
            .or_else(|| self.endpoint.clone())
    }

    fn into_facts(self) -> Vec<BackupFact> {
        let mut out = Vec::new();
        if let Some(email) = self.email {
            out.push(BackupFact {
                key: "email".into(),
                value: email,
            });
        }
        if let Some(secret_tail) = self.secret_tail {
            out.push(BackupFact {
                key: "secretTail".into(),
                value: secret_tail,
            });
        }
        if let Some(endpoint) = self.endpoint {
            out.push(BackupFact {
                key: "endpoint".into(),
                value: endpoint,
            });
        }
        if let Some(provider) = self.provider {
            out.push(BackupFact {
                key: "provider".into(),
                value: provider,
            });
        }
        if let Some(model) = self.model {
            out.push(BackupFact {
                key: "model".into(),
                value: model,
            });
        }
        out
    }
}

fn display_path_str(raw: impl AsRef<str>) -> String {
    display_path(&PathBuf::from(raw.as_ref()))
}

fn display_path(path: &Path) -> String {
    if let Ok(home) = crate::utils::paths::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display()).replace('\\', "/");
        }
    }
    path.display().to_string().replace('\\', "/")
}
