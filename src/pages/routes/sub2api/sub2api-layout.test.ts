import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

describe('sub2api layout wiring', () => {
  it('keeps three page phases and paste-token fallback in the login dialog', () => {
    const page = readFileSync(path.join(dir, 'index.tsx'), 'utf8');
    expect(page).toContain("phase === 'logged-out'");
    expect(page).toContain("phase === 'logged-in'");
    expect(page).toContain("phase === 'logging-in'");
    expect(page).toContain('openSub2ApiLoginWindow');
    expect(page).toContain('pasteToken');
    expect(page).toContain('openExternalLink');
    expect(page).toContain('syncSub2ApiKeyToConnections');
    expect(page).toContain('syncedKeysEmpty');
    expect(page).toContain('loginUrlLabel');
    expect(page).not.toContain('<iframe');
  });
});
