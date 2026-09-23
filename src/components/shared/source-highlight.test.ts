import { describe, expect, it } from 'vitest';
import { highlightDetailTokens, highlightSourceTokens } from './source-highlight';

describe('source highlight', () => {
  it('colors keywords, strings, and numbers apart', () => {
    const tokens = highlightSourceTokens('const name = "ok";\nconst n = 1;\n', 'typescript');
    expect(tokens?.find((token) => token.text === 'const')?.className).toBe('tok-keyword');
    expect(tokens?.some((token) => token.className === 'tok-string' && token.text.includes('ok'))).toBe(true);
    expect(tokens?.some((token) => token.className === 'tok-number' && token.text === '1')).toBe(true);
  });

  it('keeps diff markers and highlights keywords inside the changed line', () => {
    const tokens = highlightDetailTokens('@@ -1 +1 @@\n-const n = 0;\n+const n = 1;\n', 'src/app.ts');
    expect(tokens?.some((token) => token.text === '+' && token.className === 'tok-inserted')).toBe(true);
    expect(tokens?.some((token) => token.text === '-' && token.className === 'tok-deleted')).toBe(true);
    expect(tokens?.filter((token) => token.text === 'const').every((token) => token.className === 'tok-keyword')).toBe(true);
  });

  it('leaves prose uncolored', () => {
    expect(highlightDetailTokens('先看工作目录', 'notes.md')).toBeNull();
  });
});
