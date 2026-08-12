import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { packageAppVersion, UNKNOWN_APP_VERSION } from '@/lib/app-version';

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const packageVersion = JSON.parse(
  readFileSync(path.join(rootDir, 'package.json'), 'utf8'),
).version as string;

describe('app-version', () => {
  it('exposes a non-product fallback constant', () => {
    expect(UNKNOWN_APP_VERSION).toBe('unknown');
  });

  it('tracks package.json via Vite inject (no hard-coded semver in src)', () => {
    expect(packageAppVersion()).toBe(packageVersion);
    expect(packageAppVersion()).toMatch(/^\d+\.\d+\.\d+/);
  });
});
