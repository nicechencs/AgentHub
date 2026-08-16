/**
 * 字号契约：生产源码不得再长出第四档。
 * 三档真源见 `TYPE_SCALE`；旧名 text-sm / text-xs 等是同像素别名。
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { cn } from '@/lib/utils';

const srcRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function walkSourceFiles(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules') continue;
    const full = path.join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) {
      walkSourceFiles(full, out);
      continue;
    }
    if (/\.(ts|tsx)$/.test(name) && !/\.(test|spec)\.(ts|tsx)$/.test(name)) {
      out.push(full);
    }
  }
  return out;
}

function toPosixRel(abs: string): string {
  return path.relative(srcRoot, abs).split(path.sep).join('/');
}

/** 已退役的「第四档」名，以及任意像素字号。tokens.ts 的别名表除外。 */
const RETIRED_SIZE =
  /\btext-(?:2xs|xl|2xl|3xl|4xl|5xl|6xl|7xl|8xl|9xl)\b|text-\[\d+px\]/;
const FONT_SIZE_PROP = /fontSize:\s*(\d+)/g;
const ALLOWED_FONT_PX = new Set(['12', '13', '16']);

describe('type scale source contract', () => {
  it('does not introduce a fourth font size in production source', () => {
    const hits: string[] = [];
    for (const abs of walkSourceFiles(srcRoot)) {
      const rel = toPosixRel(abs);
      if (rel === 'styles/tokens.ts') continue;
      const src = readFileSync(abs, 'utf8');
      if (RETIRED_SIZE.test(src)) hits.push(rel);
      for (const match of src.matchAll(FONT_SIZE_PROP)) {
        if (!ALLOWED_FONT_PX.has(match[1])) hits.push(`${rel} (fontSize:${match[1]})`);
      }
    }
    expect(hits).toEqual([]);
  });

  it('keeps semantic font-size classes when merged with text color', () => {
    expect(cn('text-meta', 'text-secondary')).toContain('text-meta');
    expect(cn('text-meta', 'text-secondary')).toContain('text-secondary');
    expect(cn('text-body', 'text-primary')).toContain('text-body');
    expect(cn('text-title', 'text-primary')).toContain('text-title');
    expect(cn('text-meta', 'text-body')).toBe('text-body');
  });
});
