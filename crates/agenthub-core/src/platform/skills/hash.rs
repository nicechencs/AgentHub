//! Shallow skill-root fingerprints and streamed file-tree hashes.

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

use super::fs_safe::is_link_or_reparse;
use super::fs_index::join_normalized;

/// Shallow directory fingerprint: entry name + kind + mtime/size, plus `SKILL.md`
/// when present. Deep content is intentionally not hashed (writes + watcher
/// invalidate; nested edits usually bump parent/SKILL.md mtime on Windows).
///
/// Hash order is **sorted by entry name** so fingerprint is stable across `read_dir`.
pub(crate) fn hash_skill_root_shallow(root: &Path, hasher: &mut DefaultHasher) {
    let Ok(rd) = fs::read_dir(root) else {
        0u8.hash(hasher);
        return;
    };
    // (name, kind, mtime, len, optional SKILL.md mtime/len)
    let mut rows: Vec<(String, u8, u64, u64, Option<(u64, u64)>)> = Vec::new();
    for ent in rd.flatten() {
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = ent.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        let kind: u8 = if is_link_or_reparse(&meta) {
            1
        } else if meta.is_dir() {
            2
        } else if meta.is_file() {
            3
        } else {
            4
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let len = meta.len();
        let skill_md_fp = if kind == 2 {
            let skill_md = path.join("SKILL.md");
            fs::symlink_metadata(&skill_md).ok().map(|sm| {
                let sm_mtime = sm
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                (sm_mtime, sm.len())
            })
        } else {
            None
        };
        rows.push((name, kind, mtime, len, skill_md_fp));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, kind, mtime, len, skill_md_fp) in rows {
        name.hash(hasher);
        kind.hash(hasher);
        mtime.hash(hasher);
        len.hash(hasher);
        match skill_md_fp {
            Some((sm_mtime, sm_len)) => {
                1u8.hash(hasher);
                sm_mtime.hash(hasher);
                sm_len.hash(hasher);
            }
            None => 0u8.hash(hasher),
        }
    }
}

/// Stream-hash each file listed in `index` under `root`. Buffers are discarded
/// after hashing — only the u64 digest is kept.
pub(crate) fn hash_tree_files(
    root: &Path,
    index: &BTreeMap<String, u64>,
) -> std::result::Result<BTreeMap<String, u64>, ()> {
    let mut out = BTreeMap::new();
    for rel in index.keys() {
        let path = join_normalized(root, rel)?;
        let hash = stream_file_hash(&path)?;
        out.insert(rel.clone(), hash);
    }
    Ok(out)
}


pub(crate) fn stream_file_hash(path: &Path) -> std::result::Result<u64, ()> {
    let mut file = fs::File::open(path).map_err(|_| ())?;
    let mut hasher = DefaultHasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|_| ())?;
        if n == 0 {
            break;
        }
        hasher.write(&buf[..n]);
    }
    Ok(hasher.finish())
}
