//! ZCode usage from `cli/db/db.sqlite` `model_usage` (per-request token rows).
//! Desktop-only installs without that database yield an empty harvest.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::Result;
use crate::integrations::shared::projects::builtin_key;
use crate::integrations::shared::sqlite::{epoch_to_rfc3339, open_readonly, table_exists};
use crate::models::{AgentId, ParsedUsageEvent};
use crate::platform::usage::{UsageFileParser, UsageLineOutcome, UsageSource};
use crate::usage::grok::split_input_tokens;

struct ZcodeUsageSource;

struct NoopParser;

impl UsageFileParser for NoopParser {
    fn on_line(&mut self, _line: &str, _session_id: Option<&str>) -> UsageLineOutcome {
        UsageLineOutcome::Skipped
    }
}

impl UsageSource for ZcodeUsageSource {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("zcode")
    }

    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        Ok(Vec::new())
    }

    fn begin_file(&self, _path: &Path, _byte_offset: u64) -> Box<dyn UsageFileParser> {
        Box::new(NoopParser)
    }

    fn harvest_events(&self) -> Result<Vec<ParsedUsageEvent>> {
        let home = match crate::utils::paths::agent_home(AgentId::Zcode) {
            Ok(h) => h,
            Err(_) => return Ok(Vec::new()),
        };
        Ok(collect_zcode_usage(&home))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.usage
        .register(Arc::new(ZcodeUsageSource))
        .expect("unique built-in usage source");
}

pub(crate) fn collect_zcode_usage(home: &Path) -> Vec<ParsedUsageEvent> {
    let path = home.join("cli").join("db").join("db.sqlite");
    let Some(conn) = open_readonly(&path) else {
        return Vec::new();
    };
    if !table_exists(&conn, "model_usage") {
        return Vec::new();
    }
    let mut stmt = match conn.prepare(
        "SELECT id, session_id, model_id,
                IFNULL(input_tokens, 0),
                IFNULL(output_tokens, 0),
                IFNULL(cache_creation_input_tokens, 0),
                IFNULL(cache_read_input_tokens, 0),
                COALESCE(completed_at, started_at),
                IFNULL(status, '')
         FROM model_usage",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let iter = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, Option<i64>>(7)?,
            r.get::<_, String>(8)?,
        ))
    });
    let Ok(iter) = iter else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in iter.flatten() {
        let (id, session_id, model_id, input, output, cache_create, cache_read, ts, status) = row;
        if status != "completed" {
            continue;
        }
        if input == 0 && output == 0 && cache_create == 0 && cache_read == 0 {
            continue;
        }
        let (input, cache_read, cache_create) = split_input_tokens(input, cache_read, cache_create);
        let model = model_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown")
            .to_string();
        let ts = ts
            .map(epoch_to_rfc3339)
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        let sid = session_id
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        out.push(ParsedUsageEvent {
            agent_id: AgentId::Zcode,
            model,
            input_tokens: input,
            output_tokens: output.max(0),
            cache_creation_tokens: cache_create,
            cache_creation_1h_tokens: 0,
            cache_read_tokens: cache_read,
            session_id: sid,
            ts,
            raw_hash: format!("zcode:{id}"),
            cost_usd: None,
            fast: false,
        });
    }
    out
}

#[cfg(test)]
#[path = "usage/tests.rs"]
mod tests;
