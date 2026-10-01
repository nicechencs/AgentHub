import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import { presentLogin } from './presentation';
import type { LoginIdentityInput, LoginStatusInput } from './types';

const t = createTranslator('zh');

function identity(
  overrides: Partial<LoginIdentityInput & LoginStatusInput> = {},
): LoginIdentityInput & LoginStatusInput {
  return {
    sourceKind: 'account',
    sourceId: 'login-1',
    agentId: 'claude',
    kind: 'oauth',
    label: 'claude-oauth',
    credentialKind: 'oauth',
    authHealth: 'verified',
    ...overrides,
  };
}

describe('presentLogin identity', () => {
  it('prefers a known official identity and falls back to the safe label', () => {
    expect(presentLogin(identity({ identityLabel: 'user@example.com' }), t).identity.primary)
      .toBe('user@example.com');
    expect(presentLogin(identity({ identityLabel: '官方未提供登录信息', label: 'Claude' }), t).identity.primary)
      .toBe('Claude');
    expect(presentLogin(identity({ label: 'codex oauth' }), t).identity.primary).toBe('codex oauth');
  });

  it('shows an API Key name, type, and endpoint host without a secret', () => {
    const secret = 'sk-live-SECRETVALUE';
    const input = {
      ...identity({
        sourceKind: 'provider' as const,
        kind: 'apikey' as const,
        credentialKind: 'apikey' as const,
        label: 'OpenRouter · openrouter.ai/api/v1',
        endpointHost: 'https://openrouter.ai/api/v1',
        endpointMode: 'custom' as const,
        authHealth: 'configured' as const,
      }),
      secretTail: '**wxyz',
      token: secret,
      configText: `{"apiKey":"${secret}"}`,
      refreshToken: 'refresh-token-secret',
    };
    const view = presentLogin(input, t);
    const markup = renderToStaticMarkup(createElement(
      'div',
      null,
      view.identity.primary,
      view.identity.secondary,
      view.status.label,
    ));
    expect(view.identity.primary).toBe('OpenRouter');
    expect(view.identity.secondary).toBe('API Key · openrouter.ai');
    expect(view.kind).toBe('apikey');
    const packed = `${JSON.stringify(view)}\n${markup}`;
    expect(packed).not.toContain(secret);
    expect(packed).not.toContain('**wxyz');
    expect(packed).not.toContain('refresh-token-secret');
    expect(packed).not.toContain('configText');
    expect(packed).not.toContain('/api/v1');
  });
});

describe('presentLogin status', () => {
  it('maps health and legacy auth status to one label and tone', () => {
    expect(presentLogin(identity({ authHealth: 'verified' }), t).status).toEqual({
      label: '已验证',
      tone: 'success',
    });
    expect(presentLogin(identity({ authHealth: 'renewable' }), t).status).toEqual({
      label: '可续期',
      tone: 'success',
    });
    expect(presentLogin(identity({ authHealth: 'configured' }), t).status).toEqual({
      label: '已配置',
      tone: 'warning',
    });
    expect(presentLogin(identity({ authHealth: 'configured', catalogEmpty: true }), t).status).toEqual({
      label: '已配置 · 没有可用模型',
      tone: 'warning',
    });
    expect(presentLogin(identity({ authHealth: 'configured', inTrash: true }), t).status).toEqual({
      label: '已进回收站',
      tone: 'warning',
    });
    expect(presentLogin(identity({ authHealth: 'configured', memberUnhealthy: true }), t).status).toEqual({
      label: '登录不健康',
      tone: 'warning',
    });
    expect(presentLogin(identity({ authHealth: 'needs_login' }), t).status).toEqual({
      label: '需要重新登录',
      tone: 'danger',
    });
    expect(presentLogin(identity({ authHealth: 'unknown' }), t).status).toEqual({
      label: '状态未知',
      tone: 'muted',
    });
    expect(presentLogin(identity({ authHealth: 'missing' }), t).status).toEqual({
      label: '未登录',
      tone: 'muted',
    });
    expect(presentLogin(identity({ authHealth: undefined, authStatus: 'expired' }), t).status).toEqual({
      label: '需要重新登录',
      tone: 'danger',
    });
    expect(presentLogin(identity({ authHealth: undefined, authStatus: 'expiring' }), t).status).toEqual({
      label: '即将过期',
      tone: 'warning',
    });
    expect(presentLogin(identity({ authHealth: undefined, authStatus: 'none' }), t).status).toEqual({
      label: '未登录',
      tone: 'muted',
    });
    expect(presentLogin(identity({ authHealth: undefined, authStatus: undefined }), t).status).toEqual({
      label: '状态未知',
      tone: 'muted',
    });
  });
});

describe('login kernel imports', () => {
  it('does not import pages, the API façade, runtime, or Tauri', () => {
    const dir = path.dirname(fileURLToPath(import.meta.url));
    const files = readdirSync(dir).filter((name) => !name.includes('.test.') && name !== 'index.ts');
    expect(files.length).toBeGreaterThan(0);
    for (const name of files) {
      const src = readFileSync(path.join(dir, name), 'utf8');
      expect(src, name).not.toMatch(/@\/pages\//);
      expect(src, name).not.toMatch(/@\/lib\/api/);
      expect(src, name).not.toMatch(/@\/app\/runtime/);
      expect(src, name).not.toMatch(/@tauri-apps/);
      expect(src, name).not.toMatch(/priority/);
      expect(src, name).not.toMatch(/schedulePolicy/);
      expect(src, name).not.toMatch(/poolOwned/);
    }
  });
});
