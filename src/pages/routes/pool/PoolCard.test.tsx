import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import { PoolCard } from './PoolCard';

function profile(partial: Partial<AdapterProfile> = {}): AdapterProfile {
  return {
    id: 'bridge-1',
    name: 'Kimi → Codex',
    sourceKind: 'provider',
    sourceId: 'kimi-1',
    targetAgentId: 'codex',
    route: 'local_bridge',
    mode: 'api',
    status: 'active',
    ruleId: 'kimi-membership-to-codex-v1',
    ruleVersion: '1',
    generatedProviderId: 'codex-bridge-1',
    localPort: 43121,
    autoStart: true,
    createdAt: '2026-08-12T00:00:00Z',
    updatedAt: '2026-08-12T00:00:00Z',
    ...partial,
  };
}

describe('PoolCard', () => {
  it('renders the local entry, members, and workbench actions', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(PoolCard, {
          row: {
            key: 'bridge-1',
            pool: {
              id: 'bridge-1',
              targetAgentId: 'codex',
              surface: 'responses',
              dialect: 'codex',
              v2Enrolled: true,
              gatewayPort: 43121,
              members: [{ sourceKind: 'provider', sourceId: 'kimi-1', enabled: true }],
              listedModels: ['kimi-k2.5'],
            },
            profile: profile(),
            targetAgentId: 'codex',
            surface: 'responses',
            gatewayPort: 43121,
          },
          entries: [{
            key: 'provider:kimi-1',
            source: 'provider',
            kind: 'apikey',
            id: 'kimi-1',
            agentId: 'kimi',
            title: 'Kimi 会员',
            subtitle: '',
            isCurrent: true,
            authStatus: 'valid',
            sortKey: '',
          }],
          bridgeStatus: {
            profileId: 'bridge-1',
            state: 'running',
            port: 43121,
            endpoint: 'http://127.0.0.1:43121',
            startedAt: '2026-08-12T00:00:00Z',
            upstreamStatus: 'connected',
          },
          statusUnavailable: false,
          busy: false,
          error: undefined,
          active: false,
          targetHidden: false,
          onStart: vi.fn(),
          onStop: vi.fn(),
          onWrite: vi.fn(),
          onShowDetail: vi.fn(),
        }),
      ),
    );
    expect(markup).toContain('data-pool-card="bridge-1"');
    expect(markup).toContain('http://127.0.0.1:43121');
    expect(markup).toContain('Kimi 会员');
    expect(markup).toContain('kimi-k2.5');
    expect(markup).toContain('停止');
    expect(markup).toContain('详情');
  });
});
