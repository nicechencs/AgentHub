import { describe, expect, it } from 'vitest';
import { isPoolShareableLogin } from './ticket-pool-import';

describe('isPoolShareableLogin', () => {
  it('allows API keys from any Agent, including WorkBuddy / ZCode / Pi', () => {
    expect(isPoolShareableLogin({ agentId: 'workbuddy', credentialClass: 'api_key' })).toBe(true);
    expect(isPoolShareableLogin({ agentId: 'zcode', kind: 'apikey' })).toBe(true);
    expect(isPoolShareableLogin({ agentId: 'pi', credentialClass: 'api_key' })).toBe(true);
    expect(isPoolShareableLogin({ agentId: 'kimi', credentialClass: 'api_key' })).toBe(true);
  });

  it('allows Claude / Codex / Grok official logins and blocks other OAuth', () => {
    expect(isPoolShareableLogin({ agentId: 'claude', credentialClass: 'oauth' })).toBe(true);
    expect(isPoolShareableLogin({ agentId: 'codex', kind: 'oauth' })).toBe(true);
    expect(isPoolShareableLogin({ agentId: 'kimi', credentialClass: 'oauth' })).toBe(false);
    expect(isPoolShareableLogin({ agentId: 'workbuddy', kind: 'oauth' })).toBe(false);
  });
});
