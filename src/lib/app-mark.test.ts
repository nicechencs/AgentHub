import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { APP_MARK_PATH, appMarkSvg } from './app-mark';

describe('app mark svg', () => {
  it('paints the tile with the given fill and the shared glyph path', () => {
    const svg = appMarkSvg('#2563eb');
    expect(svg).toContain('fill="#2563eb"');
    expect(svg).toContain(APP_MARK_PATH);
    expect(svg).not.toContain('#4f46e5');
  });

  it('stays in lockstep with the in-app AppLogo glyph', () => {
    const logo = readFileSync(
      path.join(path.dirname(fileURLToPath(import.meta.url)), '../components/shared/AppLogo.tsx'),
      'utf8',
    );
    expect(logo).toContain(APP_MARK_PATH);
  });
});
