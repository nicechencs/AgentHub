/**
 * Normalize filesystem paths for "open in file manager" actions.
 *
 * Handles path-format drift from project discovery (e.g. `cwd/D:/work`,
 * forward-slash Windows paths) so UI only offers open when the path looks
 * like a real absolute location.
 */

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
 * Prefer workspace (actualPath), then native storage dir.
 * Returns candidates already normalized; empty if none openable.
 */
export function projectOpenCandidates(paths: {
  actualPath?: string | null;
  storagePath?: string | null;
}): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const raw of [paths.actualPath, paths.storagePath]) {
    const n = normalizeOpenPath(raw);
    if (!n || seen.has(n.toLowerCase())) continue;
    seen.add(n.toLowerCase());
    out.push(n);
  }
  return out;
}
