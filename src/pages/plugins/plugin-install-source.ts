/** Pi runs package commands from an isolated directory, so local sources must be absolute. */
export function isPiAbsoluteLocalSource(raw: string): boolean {
  const source = raw.trim();
  if (!source || source.includes('\0')) return false;
  return (
    source.startsWith('/') ||
    source === '~' ||
    source.startsWith('~/') ||
    source.startsWith('~\\') ||
    /^[A-Za-z]:[\\/]/.test(source) ||
    source.startsWith('\\\\')
  );
}
