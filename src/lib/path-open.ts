/**
 * Normalize filesystem paths for "open in file manager" actions.
 *
 * Handles path-format drift from project discovery (e.g. `cwd/D:/work`,
 * forward-slash Windows paths, Claude `-C-Users-…` dir names) so UI only
 * offers open when the path looks like a real absolute location.
 */

/** Agents that store workspaces as Claude-style `-C-Users-foo` directory names. */
const CLAUDE_ENCODED_AGENTS = new Set(['claude', 'workbuddy']);

/** True if `p` looks like an absolute local path (Windows drive / UNC / Unix). */
export function isAbsoluteFsPath(p: string): boolean {
  const t = p.trim();
  if (!t) return false;
  // Windows drive: C:\ or C:/
  if (/^[A-Za-z]:[\\/]/.test(t)) return true;
  // UNC: \\server\share
  if (t.startsWith('\\\\')) return true;
  // Unix absolute
  if (t.startsWith('/') && !t.startsWith('//')) return true;
  return false;
}

/**
 * Normalize a recorded path into an absolute OS-friendly form, or null if it
 * cannot be used to open a folder (relative keys, empty, opaque ids).
 *
 * - Strips accidental `cwd/` storage-key prefix from project grouping keys
 * - Windows: `/` → `\`
 * - Rejects bare relative segments and encoded Claude-style dir names alone
 */
export function normalizeOpenPath(raw?: string | null): string | null {
  if (raw == null) return null;
  let p = raw.trim();
  if (!p) return null;

  // Project storage keys sometimes leak into path fields: `cwd/D:/work/repo`
  if (p.startsWith('cwd/')) {
    p = p.slice(4);
  }
  // `dir/<opaque>` Grok buckets are not openable as workspaces
  if (p.startsWith('dir/')) {
    return null;
  }
  if (p === '__ungrouped__') {
    return null;
  }

  // Windows drive with forward slashes → backslashes
  if (/^[A-Za-z]:[\\/]/.test(p) || p.startsWith('\\\\')) {
    p = p.replace(/\//g, '\\');
  }

  if (!isAbsoluteFsPath(p)) {
    return null;
  }
  return p;
}

/**
 * True when `name` looks like a Claude / WorkBuddy encoded project dir
 * (`-C-Users-demo-app`, `-Users-foo-bar`), not a cwd key or opaque bucket.
 */
export function looksLikeClaudeEncodedDir(name: string): boolean {
  const n = name.trim();
  if (!n.startsWith('-')) return false;
  return n.split('-').filter(Boolean).length >= 2;
}

/**
 * Restore a Claude / WorkBuddy encoded dir name to a filesystem path.
 * Mirrors `decode_claude_project_dir` in core; caller must still validate format.
 */
export function decodeClaudeProjectDir(encoded: string): string | null {
  const s = encoded.trim();
  if (!s) return null;

  if (s.startsWith('-')) {
    const rest = s.slice(1);
    const parts = rest.split('-').filter((p) => p.length > 0);
    const drive = parts[0];
    if (drive && drive.length === 1 && /[A-Za-z]/.test(drive)) {
      return `${drive.toUpperCase()}:\\${parts.slice(1).join('\\')}`;
    }
    const joined = parts.join('/');
    if (joined) return `/${joined}`;
  }

  return s.replace(/-/g, '/');
}

function lastPathSegment(raw?: string | null): string {
  if (!raw) return '';
  const parts = raw.trim().split(/[/\\]/).filter(Boolean);
  return parts[parts.length - 1] ?? '';
}

export function claudeEncodedDirName(paths: {
  relativePath?: string | null;
  storagePath?: string | null;
}): string | null {
  for (const raw of [paths.relativePath, paths.storagePath]) {
    const name = lastPathSegment(raw);
    if (looksLikeClaudeEncodedDir(name)) return name;
  }
  return null;
}

export type ProjectPathFields = {
  agentId?: string | null;
  actualPath?: string | null;
  relativePath?: string | null;
  storagePath?: string | null;
};

/**
 * Display restore: format-valid actualPath, or a decoded Claude dir name.
 * Not proof the folder exists — do not use this to enable "open".
 */
export function restoreProjectWorkspacePath(paths: ProjectPathFields): string | null {
  const actual = normalizeOpenPath(paths.actualPath);
  if (actual) return actual;

  const agent = paths.agentId?.trim();
  if (agent && !CLAUDE_ENCODED_AGENTS.has(agent)) return null;

  const encoded = claudeEncodedDirName(paths);
  if (!encoded) return null;
  return normalizeOpenPath(decodeClaudeProjectDir(encoded));
}

/**
 * Openable workspace: only a format-valid `actualPath` from the backend
 * (decoded and verified to exist on disk). Client-side restore is not enough.
 */
export function verifiedProjectWorkspacePath(paths: ProjectPathFields): string | null {
  return normalizeOpenPath(paths.actualPath);
}

/**
 * Prefer a verified workspace, then the native storage dir.
 * Returns candidates already normalized; empty if none openable.
 */
export function projectOpenCandidates(paths: ProjectPathFields): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  const add = (raw?: string | null) => {
    const n = normalizeOpenPath(raw);
    if (!n || seen.has(n.toLowerCase())) return;
    seen.add(n.toLowerCase());
    out.push(n);
  };
  add(verifiedProjectWorkspacePath(paths));
  add(paths.storagePath);
  return out;
}
