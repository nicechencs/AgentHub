import { describe, expect, it } from 'vitest';
import type { AgentKey } from '@/lib/types';
import {
  buildConnectionsGuideUrl,
  buildResumeConnectUrl,
  consumeConnectIntent,
  consumeConnectResume,
  parseConnectGuideIntent,
  parseConnectResumeParam,
  parseResumeAgentId,
  readConnectGuide,
} from './connect-intent';

const ALLOWED = ['claude', 'codex', 'kimi'] as const satisfies readonly AgentKey[];

function splitUrl(url: string): { path: string; search: URLSearchParams } {
  const q = url.indexOf('?');
  return {
    path: q === -1 ? url : url.slice(0, q),
    search: new URLSearchParams(q === -1 ? '' : url.slice(q + 1)),
  };
}

describe('parseConnectGuideIntent', () => {
  it('accepts all guide intents', () => {
    expect(parseConnectGuideIntent('import-login')).toBe('import-login');
    expect(parseConnectGuideIntent('add-key')).toBe('add-key');
    expect(parseConnectGuideIntent('oauth')).toBe('oauth');
  });

  it('returns null for illegal, empty, or missing intent', () => {
    expect(parseConnectGuideIntent(null)).toBeNull();
    expect(parseConnectGuideIntent(undefined)).toBeNull();
    expect(parseConnectGuideIntent('')).toBeNull();
    expect(parseConnectGuideIntent('import_login')).toBeNull();
  });
});

describe('parseResumeAgentId / parseConnectResumeParam', () => {
  it('returns the agent when it is in allowed', () => {
    expect(parseResumeAgentId('claude', ALLOWED)).toBe('claude');
    expect(parseConnectResumeParam('codex', ALLOWED)).toBe('codex');
  });

  it('returns null for empty, missing, or unknown agent', () => {
    expect(parseResumeAgentId(null, ALLOWED)).toBeNull();
    expect(parseResumeAgentId(undefined, ALLOWED)).toBeNull();
    expect(parseResumeAgentId('', ALLOWED)).toBeNull();
    expect(parseResumeAgentId('grok', ALLOWED)).toBeNull();
    expect(parseConnectResumeParam('pi', ALLOWED)).toBeNull();
    expect(parseConnectResumeParam('', ALLOWED)).toBeNull();
  });
});

describe('buildConnectionsGuideUrl', () => {
  it('builds import-login without mode', () => {
    const url = buildConnectionsGuideUrl({
      agentId: 'claude',
      intent: 'import-login',
      resumeAgentId: 'claude',
    });
    const { path, search } = splitUrl(url);

    expect(path).toBe('/connections');
    expect(search.get('agent')).toBe('claude');
    expect(search.get('intent')).toBe('import-login');
    expect(search.get('resume')).toBe('claude');
    expect(search.has('mode')).toBe(false);
    expect(url).toBe('/connections?agent=claude&intent=import-login&resume=claude');
  });

  it('builds add-key with mode=providers', () => {
    const url = buildConnectionsGuideUrl({
      agentId: 'codex',
      intent: 'add-key',
      resumeAgentId: 'codex',
    });
    const { path, search } = splitUrl(url);

    expect(path.startsWith('/connections')).toBe(true);
    expect(search.get('agent')).toBe('codex');
    expect(search.get('mode')).toBe('providers');
    expect(search.get('intent')).toBe('add-key');
    expect(search.get('resume')).toBe('codex');
    expect(url).toBe('/connections?agent=codex&mode=providers&intent=add-key&resume=codex');
  });

  it('builds oauth without mode and without resume', () => {
    const url = buildConnectionsGuideUrl({ agentId: 'claude', intent: 'oauth' });
    const { path, search } = splitUrl(url);

    expect(path).toBe('/connections');
    expect(search.get('agent')).toBe('claude');
    expect(search.get('intent')).toBe('oauth');
    expect(search.has('mode')).toBe(false);
    expect(search.has('resume')).toBe(false);
    expect(url).toBe('/connections?agent=claude&intent=oauth');
  });

  it('omits resume when it is empty', () => {
    const url = buildConnectionsGuideUrl({
      agentId: 'codex',
      intent: 'import-login',
      resumeAgentId: null,
    });
    expect(splitUrl(url).search.has('resume')).toBe(false);
  });
});

describe('buildResumeConnectUrl', () => {
  it('builds the dashboard resume deep link', () => {
    expect(buildResumeConnectUrl('claude')).toBe('/?connect=claude');
  });
});

describe('readConnectGuide', () => {
  it('reads a valid import-login guide', () => {
    const search = new URLSearchParams('agent=claude&intent=import-login&resume=claude');
    expect(readConnectGuide(search, ALLOWED)).toEqual({
      intent: 'import-login',
      resumeAgentId: 'claude',
    });
  });

  it('reads a valid oauth guide without resume', () => {
    const search = new URLSearchParams('agent=claude&intent=oauth');
    expect(readConnectGuide(search, ALLOWED)).toEqual({
      intent: 'oauth',
      resumeAgentId: null,
    });
  });

  it('returns null when intent is illegal or missing', () => {
    expect(readConnectGuide(new URLSearchParams('resume=claude'), ALLOWED)).toBeNull();
    expect(readConnectGuide(new URLSearchParams('intent=connect&resume=claude'), ALLOWED)).toBeNull();
    expect(readConnectGuide(new URLSearchParams('intent=&resume=claude'), ALLOWED)).toBeNull();
  });

  it('keeps intent when resume is not in allowed', () => {
    const search = new URLSearchParams('agent=claude&intent=add-key&mode=providers&resume=grok');
    expect(readConnectGuide(search, ALLOWED)).toEqual({
      intent: 'add-key',
      resumeAgentId: null,
    });
  });
});

describe('consumeConnectIntent', () => {
  it('removes intent and keeps resume/agent/mode', () => {
    const search = new URLSearchParams(
      'agent=codex&mode=providers&intent=add-key&resume=codex',
    );
    const next = consumeConnectIntent(search);

    expect(next.get('intent')).toBeNull();
    expect(next.get('agent')).toBe('codex');
    expect(next.get('mode')).toBe('providers');
    expect(next.get('resume')).toBe('codex');
    expect(search.get('intent')).toBe('add-key');
  });
});

describe('consumeConnectResume', () => {
  it('removes connect and keeps other params', () => {
    const search = new URLSearchParams('connect=claude&tab=overview');
    const next = consumeConnectResume(search);

    expect(next.get('connect')).toBeNull();
    expect(next.get('tab')).toBe('overview');
    expect(search.get('connect')).toBe('claude');
  });
});
