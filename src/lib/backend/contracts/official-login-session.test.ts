import { describe, expect, it } from 'vitest';
import { flattenKeys, translate } from '@/lib/i18n';
import type { MessageKey } from '@/lib/i18n';
import { zh } from '@/lib/i18n/locales/zh';
import { en } from '@/lib/i18n/locales/en';
import {
  IMPLEMENTED_OFFICIAL_LOGIN_IDS,
  UNIMPLEMENTED_PI_OAUTH_KEYS,
  isImplementedOfficialLogin,
  isUnimplementedPiOauth,
  mapDevicePollStatus,
  mapPkceWaitStatus,
  officialLoginAdapter,
  officialLoginCopyId,
  officialLoginCopyLeaksInternals,
  officialLoginFooter,
  officialLoginRetryStep,
  officialLoginShouldFinish,
  officialLoginShouldKeepPolling,
  officialLoginSuccessView,
  presentOfficialLoginOptions,
  sessionFromDeviceStart,
  sessionFromPkceStart,
  validateManualCallbackUrl,
} from './official-login-session';

describe('official login option catalog', () => {
  it('lists only implemented Claude / Codex / Grok / Pi options', () => {
    expect(IMPLEMENTED_OFFICIAL_LOGIN_IDS.claude).toEqual(['claude']);
    expect(IMPLEMENTED_OFFICIAL_LOGIN_IDS.codex).toEqual(['codex']);
    expect(IMPLEMENTED_OFFICIAL_LOGIN_IDS.grok).toEqual(['xai']);
    expect(IMPLEMENTED_OFFICIAL_LOGIN_IDS.pi).toEqual(['anthropic', 'openai-codex', 'xai']);
    expect(IMPLEMENTED_OFFICIAL_LOGIN_IDS.kimi).toBeUndefined();
    expect(IMPLEMENTED_OFFICIAL_LOGIN_IDS.dsh).toBeUndefined();
    expect(IMPLEMENTED_OFFICIAL_LOGIN_IDS.cursor).toBeUndefined();
  });

  it('keeps known-but-unimplemented Pi keys off the clickable list', () => {
    for (const key of UNIMPLEMENTED_PI_OAUTH_KEYS) {
      expect(isUnimplementedPiOauth(key)).toBe(true);
      expect(isImplementedOfficialLogin('pi', key)).toBe(false);
      expect(officialLoginCopyId('pi', key)).toBeNull();
    }
    const shown = presentOfficialLoginOptions([
      { agentId: 'pi', id: 'anthropic' },
      { agentId: 'pi', id: 'github-copilot' },
      { agentId: 'pi', id: 'openrouter' },
      { agentId: 'pi', id: 'kimi-coding' },
      { agentId: 'pi', id: 'radius' },
      { agentId: 'pi', id: 'xai' },
      { agentId: 'kimi', id: 'kimi' },
    ]);
    expect(shown.map((row) => row.id)).toEqual(['anthropic', 'xai']);
  });

  it('maps implemented aliases to copy ids without inventing new vendors', () => {
    expect(officialLoginCopyId('claude', 'claude')).toBe('claude');
    expect(officialLoginCopyId('codex', 'openai-codex')).toBe('codex');
    expect(officialLoginCopyId('grok', 'xai')).toBe('grok');
    expect(officialLoginCopyId('pi', 'openai')).toBe('piCodex');
    expect(officialLoginCopyId('pi', 'grok')).toBe('piXai');
    expect(officialLoginAdapter('deviceCode')).toBe('deviceCode');
    expect(officialLoginAdapter('pkce')).toBe('pkce');
  });
});

describe('official login status mapping', () => {
  it('maps PKCE wait statuses onto the shared wait-page phases', () => {
    expect(mapPkceWaitStatus('waiting')).toBe('waiting');
    expect(mapPkceWaitStatus('callbackReceived')).toBe('ready');
    expect(mapPkceWaitStatus('succeeded')).toBe('ready');
    expect(mapPkceWaitStatus('failed')).toBe('failed');
    expect(officialLoginShouldFinish('ready')).toBe(true);
    expect(officialLoginShouldKeepPolling('waiting')).toBe(true);
    expect(officialLoginShouldKeepPolling('ready')).toBe(false);
  });

  it('maps device-code poll statuses onto the same phases', () => {
    expect(mapDevicePollStatus('pending')).toBe('waiting');
    expect(mapDevicePollStatus('slowDown')).toBe('waiting');
    expect(mapDevicePollStatus('completing')).toBe('waiting');
    expect(mapDevicePollStatus('complete')).toBe('ready');
    expect(mapDevicePollStatus('failed')).toBe('failed');
    expect(mapDevicePollStatus('expired')).toBe('expired');
  });

  it('builds a shared session from either start payload without a third grant type', () => {
    const pkce = sessionFromPkceStart(
      {
        state: 'pkce-state',
        authorizeUrl: 'https://example.test/auth',
        redirectUri: 'http://127.0.0.1:1455/callback',
        agentId: 'claude',
        providerKey: null,
        browserOpened: true,
      },
      'claude',
    );
    expect(pkce).toMatchObject({
      sessionId: 'pkce-state',
      agentId: 'claude',
      optionId: 'claude',
      flow: 'pkce',
    });

    const device = sessionFromDeviceStart({
      state: 'device-state',
      agentId: 'pi',
      providerKey: 'xai',
      userCode: 'ABCD-EFGH',
      verificationUri: 'https://auth.x.ai/device',
      verificationUriComplete: 'https://auth.x.ai/device?user_code=ABCD-EFGH',
      intervalSecs: 5,
      expiresInSecs: 900,
    });
    expect(device).toMatchObject({
      sessionId: 'device-state',
      optionId: 'xai',
      flow: 'deviceCode',
      userCode: 'ABCD-EFGH',
    });
  });

  it('makes retry the primary footer after failure', () => {
    expect(officialLoginRetryStep(3)).toBe('pick');
    expect(officialLoginRetryStep(1)).toBe('start');
    expect(officialLoginFooter('error', false)).toBe('retry');
    expect(officialLoginFooter('waiting', false)).toBe('cancelWait');
    expect(officialLoginFooter('done', false)).toBe('success');
    expect(officialLoginFooter('start', false)).toBe('close');
  });
});

describe('official login success identity', () => {
  it('does not invent email, last4, or identity when the login omitted them', () => {
    expect(
      officialLoginSuccessView({
        label: 'claude-oauth',
        email: undefined,
        identityLabel: undefined,
        subscription: undefined,
        secretTail: '**JF6Q',
      }),
    ).toEqual({ title: null, subscription: null, identity: null });
  });

  it('shows only fields the login actually returned', () => {
    expect(
      officialLoginSuccessView({
        label: 'pi:anthropic · ada@example.com',
        email: 'ada@example.com',
        identityLabel: 'ada@example.com',
        subscription: 'Claude Pro',
        secretTail: '**AB12',
      }),
    ).toEqual({
      title: 'ada@example.com',
      subscription: 'Claude Pro',
      identity: null,
    });
  });
});

describe('validateManualCallbackUrl', () => {
  const redirect = 'http://127.0.0.1:1455/callback';
  const state = 'abc';

  it('accepts a loopback callback with matching state and code', () => {
    const got = validateManualCallbackUrl(`${redirect}?code=ok&state=${state}`, redirect, state);
    expect(got.ok).toBe(true);
  });

  it('rejects a public URL even when it contains code=', () => {
    expect(
      validateManualCallbackUrl(
        `https://evil.example/steal?code=ok&state=${state}`,
        redirect,
        state,
      ).ok,
    ).toBe(false);
  });

  it('rejects a loopback URL with the wrong state', () => {
    expect(
      validateManualCallbackUrl(`${redirect}?code=ok&state=other`, redirect, state).ok,
    ).toBe(false);
  });
});

describe('official login user copy', () => {
  it('keeps zh/en wait-page copy free of internals', () => {
    const keys = flattenKeys(zh).filter((key) => key.startsWith('connect.oauth.'));
    expect(keys.length).toBeGreaterThan(10);
    for (const key of keys) {
      expect(officialLoginCopyLeaksInternals(translate('zh', key as MessageKey)), key).toBe(false);
      expect(officialLoginCopyLeaksInternals(translate('en', key as MessageKey)), key).toBe(false);
    }
    expect(translate('zh', 'connect.oauth.pickHint')).not.toContain('auth.json');
    expect(translate('zh', 'connect.oauth.writtenPi')).not.toContain('auth.json');
    expect(translate('en', 'connect.oauth.writtenPi')).not.toContain('auth.json');
    expect(lookupLeaf(zh, 'connect.oauth.expectedPrefix')).toBeUndefined();
    expect(lookupLeaf(en, 'connect.oauth.option.piXai.label')).toBe('xAI (Grok subscription)');
  });
});

function lookupLeaf(obj: unknown, key: string): string | undefined {
  let cur: unknown = obj;
  for (const part of key.split('.')) {
    if (cur == null || typeof cur !== 'object' || !(part in cur)) return undefined;
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === 'string' ? cur : undefined;
}
