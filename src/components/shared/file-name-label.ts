/** Split `~/.workbuddy/models.json` into directory prefix + file name. */
export function splitFileLabel(path: string, name = ''): {
  directory: string;
  fileName: string;
} {
  const fileName = name.trim() || baseName(path);
  const pathTrim = path.trim();
  if (!pathTrim) return { directory: '', fileName };
  if (
    pathTrim === fileName
    || pathTrim.endsWith(`/${fileName}`)
    || pathTrim.endsWith(`\\${fileName}`)
  ) {
    return { directory: pathTrim.slice(0, pathTrim.length - fileName.length), fileName };
  }
  const slash = Math.max(pathTrim.lastIndexOf('/'), pathTrim.lastIndexOf('\\'));
  if (slash >= 0) {
    return {
      directory: pathTrim.slice(0, slash + 1),
      fileName: pathTrim.slice(slash + 1),
    };
  }
  return { directory: '', fileName: pathTrim };
}

/**
 * Tail-first path label for narrow chrome: keep the last `keep` segments and
 * mark the dropped head with `…`.
 * `D:\demo\chen\2026\AgentHub` → `…\2026\AgentHub`; a path that short stays whole.
 */
export function pathTailLabel(path: string, keep = 2): string {
  const raw = path.trim();
  if (!raw) return '';
  const win = /^[A-Za-z]:/.test(raw) || raw.includes('\\');
  const sep = win ? '\\' : '/';
  const normalized = win ? raw.replace(/\//g, '\\') : raw.replace(/\\/g, '/');
  const drive = win ? /^[A-Za-z]:/.exec(normalized)?.[0] ?? '' : '';
  const parts = normalized.slice(drive.length).split(/[\\/]+/).filter(Boolean);
  const width = Math.max(1, Math.trunc(keep));
  if (parts.length <= width) return normalized;
  return `…${sep}${parts.slice(-width).join(sep)}`;
}

function baseName(path: string): string {
  const slash = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return slash >= 0 ? path.slice(slash + 1) : path;
}
