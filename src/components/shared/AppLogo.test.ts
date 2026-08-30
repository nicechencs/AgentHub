import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { appIconUrl } from './AppLogo';

const source = readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)), 'AppLogo.tsx'), 'utf8');

describe('appIconUrl', () => {
  it('resolves under Vite BASE_URL', () => {
    expect(appIconUrl()).toMatch(/app-icon\.png$/);
    expect(appIconUrl()).toContain('app-icon.png');
  });
});

describe('AppLogo', () => {
  it('paints the in-app mark with currentColor from --accent, not a fixed hex', () => {
    expect(source).toContain('fill="currentColor"');
    expect(source).toContain("color: 'var(--accent)'");
    expect(source).toContain('data-app-logo');
    expect(source).not.toContain('#4f46e5');
  });
});
