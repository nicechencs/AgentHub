import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

describe('sub2api layout wiring', () => {
  it('uses native login form; webview open-login is not the primary UX', () => {
    const page = readFileSync(path.join(dir, 'index.tsx'), 'utf8');
    expect(page).toContain("phase === 'logged-out'");
    expect(page).toContain("phase === 'logged-in'");
    expect(page).toContain("phase === 'awaiting-2fa'");
    expect(page).toContain('nativeSub2ApiLogin');
    expect(page).toContain('nativeSub2ApiLogin2FA');
    expect(page).toContain('data-sub2api-login-form');
    expect(page).toContain('data-sub2api-2fa-form');
    expect(page).toContain('data-sub2api-email');
    expect(page).toContain('data-sub2api-password');
    expect(page).toContain('data-sub2api-totp');
    expect(page).toContain('openSiteInBrowser');
    expect(page).toContain('data-sub2api-advanced');
    expect(page).toContain('pasteToken');
    expect(page).toContain('Sub2ApiCaptcha');
    expect(page).toContain('syncSub2ApiKeyToConnections');
    expect(page).not.toContain('openSub2ApiLoginWindow');
    expect(page).not.toContain('closeSub2ApiLoginWindow');
    expect(page).not.toContain('<iframe');
  });
});
