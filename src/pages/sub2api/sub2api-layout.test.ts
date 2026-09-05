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
    expect(page).toContain("phase === 'restoring'");
    expect(page).toContain('data-sub2api-remember');
    expect(page).toContain('data-sub2api-remembered-list');
    expect(page).toContain('saveRememberedAccount');
    expect(page).toContain('ensureSub2ApiSessionFresh');
    expect(page).toContain('nativeSub2ApiLogin');
    expect(page).toContain('nativeSub2ApiLogin2FA');
    expect(page).toContain('data-sub2api-login-form');
    expect(page).toContain('data-sub2api-2fa-form');
    expect(page).toContain('data-sub2api-email');
    expect(page).toContain('data-sub2api-password');
    expect(page).toContain('data-sub2api-totp');
    expect(page).toContain('data-sub2api-site-picker');
    expect(page).toContain('saveRememberedSite');
    expect(page).not.toContain('openSiteInBrowser');
    expect(page).not.toContain('data-sub2api-advanced');
    expect(page).not.toContain('pasteToken');
    expect(page).toContain('Sub2ApiCaptcha');
    expect(page).toContain('syncSub2ApiKeyToConnections');
    expect(page).toContain('data-sub2api-keys-table');
    expect(page).toContain("t('routes.sub2api.colName')");
    expect(page).toContain("t('routes.sub2api.colApiKey')");
    expect(page).toContain("t('routes.sub2api.colGroup')");
    expect(page).toContain("t('routes.sub2api.colConcurrency')");
    expect(page).toContain("t('routes.sub2api.colUsage')");
    expect(page).toContain("t('routes.sub2api.colExpires')");
    expect(page).toContain("t('routes.sub2api.colStatus')");
    expect(page).toContain("t('routes.sub2api.colCreated')");
    expect(page).toContain("t('routes.sub2api.colActions')");
    expect(page).not.toContain('openSub2ApiLoginWindow');
    expect(page).not.toContain('closeSub2ApiLoginWindow');
    expect(page).not.toContain('<iframe');
  });
});

describe('sub2api primary route', () => {
  it('mounts at /sub2api and redirects the old routes path', () => {
    const app = readFileSync(path.resolve(dir, '../../App.tsx'), 'utf8');
    expect(app).toContain("from '@/pages/sub2api'");
    expect(app).toContain('path={SUB2API_PATH}');
    expect(app).toContain('path={ROUTES_SUB2API_PATH}');
    expect(app).toContain('to={SUB2API_PATH}');
    expect(app).toContain('pathname === SUB2API_PATH');
    expect(app).not.toContain('path="sub2api"');
    expect(app).not.toContain("@/pages/routes/sub2api");
  });
});
