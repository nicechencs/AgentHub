/** Word-safe truncation: never cut mid-word, mid-email, or mid-CJK-word. */

const SEGMENT_SEPARATORS = [' · ', '·', ' ', '/', '.'] as const;

function isCjkCodePoint(code: number): boolean {
  return (
    (code >= 0x3400 && code <= 0x4dbf)
    || (code >= 0x4e00 && code <= 0x9fff)
    || (code >= 0x3040 && code <= 0x30ff)
    || (code >= 0xac00 && code <= 0xd7af)
  );
}

function isAsciiWordChar(ch: string): boolean {
  return /[A-Za-z0-9._+-]/.test(ch);
}

function lastSeparatorIndex(text: string): number {
  const triple = text.lastIndexOf(' · ');
  if (triple > 0) return triple;
  let best = -1;
  for (const sep of SEGMENT_SEPARATORS) {
    if (sep === ' · ') continue;
    const idx = text.lastIndexOf(sep);
    if (idx > best) best = idx;
  }
  return best;
}

function stripTrailingSeparators(text: string): string {
  return text.replace(/[\s·]+$/u, '').trimEnd();
}

/**
 * Trim `text` to at most `maxLen` code points without cutting inside a word,
 * email local/domain, or CJK run. Prefers dropping the last ` · ` / space / `/`
 * segment (so "本机路由 · cunser@…" becomes "本机路由", not "本机路由 · cunse").
 */
export function truncateAtWord(text: string, maxLen: number): string {
  if (maxLen <= 0) return '';
  const chars = [...text];
  if (chars.length <= maxLen) return text;

  const slice = chars.slice(0, maxLen).join('');
  const sep = lastSeparatorIndex(slice);
  if (sep > 0) {
    return stripTrailingSeparators(slice.slice(0, sep));
  }

  const at = slice.lastIndexOf('@');
  if (at >= 0) {
    let start = at;
    while (start > 0 && isAsciiWordChar(slice[start - 1] ?? '')) start -= 1;
    if (start > 0) return slice.slice(0, start).trimEnd();
    return '';
  }

  const last = chars[maxLen - 1] ?? '';
  const next = chars[maxLen] ?? '';
  if (isAsciiWordChar(last) && isAsciiWordChar(next)) {
    let i = maxLen - 1;
    while (i >= 0 && isAsciiWordChar(chars[i] ?? '')) i -= 1;
    if (i >= 0) return chars.slice(0, i + 1).join('').trimEnd();
    return '';
  }

  const lastCode = last.codePointAt(0);
  const nextCode = next.codePointAt(0);
  if (
    lastCode !== undefined
    && nextCode !== undefined
    && isCjkCodePoint(lastCode)
    && isCjkCodePoint(nextCode)
  ) {
    let i = maxLen - 1;
    while (i >= 0 && isCjkCodePoint(chars[i]?.codePointAt(0) ?? -1)) i -= 1;
    if (i >= 0) return chars.slice(0, i + 1).join('').trimEnd();
    return chars.slice(0, maxLen).join('');
  }

  return slice.trimEnd();
}
