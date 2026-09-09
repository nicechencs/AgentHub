import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

describe('shortcut kbd', () => {
  it('uses the overlay chip and an Enter icon without the word Enter', () => {
    const src = readFileSync(path.join(dir, 'shortcut-kbd.tsx'), 'utf8');
    expect(src).toContain('CornerDownLeft');
    expect(src).toContain('border-border bg-subtle text-muted');
    expect(src).toContain('border-white/35 bg-white/15 text-white');
    expect(src).not.toMatch(/>Enter</);
    expect(src).not.toContain('删除确认');
  });
});
