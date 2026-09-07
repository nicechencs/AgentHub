/** Last path segment for a folder chosen in the OS file manager. */
export function folderNameFromCwd(cwd: string): string {
  const trimmed = cwd.trim().replace(/[\\/]+$/, '');
  if (!trimmed) return '';
  const parts = trimmed.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || trimmed;
}
