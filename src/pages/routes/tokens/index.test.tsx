import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ConnectApiKeyDraft } from '@/lib/connect-flow/connect-intent';
import type { LocalTokenRow } from './tokens-model';

const testState = vi.hoisted(() => ({
  stateSlots: [] as unknown[],
  stateIndex: 0,
  tokenListProps: null as Record<string, unknown> | null,
  detailProps: null as Record<string, unknown> | null,
  providerProps: null as Record<string, unknown> | null,
  importButtonProps: null as Record<string, unknown> | null,
  inspect: {
    target: null as string | null,
    paneWidth: 420,
    expanded: false,
    open: vi.fn((target: string) => { testState.inspect.target = target; }),
    close: vi.fn(() => { testState.inspect.target = null; }),
  },
  setLocalToken: vi.fn(),
}));

vi.mock('react', async () => {
  const actual = await vi.importActual<typeof import('react')>('react');
  return {
    ...actual,
    useState: <T,>(initial: T) => {
      const index = testState.stateIndex++;
      if (!(index in testState.stateSlots)) testState.stateSlots[index] = initial;
      return [
        testState.stateSlots[index] as T,
        (next: T | ((current: T) => T)) => {
          const current = testState.stateSlots[index] as T;
          testState.stateSlots[index] = typeof next === 'function'
            ? (next as (current: T) => T)(current)
            : next;
        },
      ] as const;
    },
    useMemo: <T,>(factory: () => T) => factory(),
    useCallback: <T extends (...args: never[]) => unknown>(callback: T) => callback,
    useEffect: () => undefined,
  };
});

vi.mock('react-router-dom', () => ({
  useNavigate: () => vi.fn(),
}));

vi.mock('lucide-react', () => ({
  KeyRound: () => null,
  Plus: () => null,
  Sparkles: () => null,
}));

vi.mock('@/components/layout/PageHeader', () => ({ PageHeader: () => null }));
vi.mock('@/components/layout/PageSection', () => ({
  PageSection: ({ children }: { children?: ReactNode }) => children ?? null,
}));
vi.mock('@/components/layout/SideSplit', () => ({
  WorkbenchSplitPage: ({ children, panel }: { children?: ReactNode; panel?: ReactNode }) => panel ?? children ?? null,
}));
vi.mock('@/components/layout/use-side-split', () => ({
  useSideSplit: () => testState.inspect,
}));
vi.mock('@/components/shared/EmptyState', () => ({ EmptyState: () => null }));
vi.mock('@/components/shared/ErrorState', () => ({ ErrorState: () => null }));
vi.mock('@/components/shared/Notice', () => ({ Notice: () => null }));
vi.mock('@/components/shared/PageRefreshButton', () => ({ PageRefreshButton: () => null }));
vi.mock('@/components/shared/LanguageProvider', () => ({
  useI18n: () => ({
    lang: 'zh',
    t: (key: string) => key,
  }),
}));
vi.mock('@/components/ui/button', () => ({
  Button: ({ children }: { children?: ReactNode }) => children ?? null,
}));
vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ open, children }: { open?: boolean; children?: ReactNode }) => open ? children : null,
  DialogContent: ({ children }: { children?: ReactNode }) => children ?? null,
  DialogDescription: ({ children }: { children?: ReactNode }) => children ?? null,
  DialogFooter: ({ children }: { children?: ReactNode }) => children ?? null,
  DialogHeader: ({ children }: { children?: ReactNode }) => children ?? null,
  DialogTitle: ({ children }: { children?: ReactNode }) => children ?? null,
}));
vi.mock('@/components/ui/input', () => ({ Input: () => null }));
vi.mock('@/components/ui/toast', () => ({ useToast: () => ({ toast: vi.fn() }) }));
vi.mock('@/components/connections/ApiKeyAccountDialog', () => ({ ApiKeyAccountDialog: () => null }));
vi.mock('@/components/connections/ProviderEditDialog', () => ({
  ProviderEditDialog: (props: Record<string, unknown>) => {
    testState.providerProps = props;
    return null;
  },
}));

vi.mock('@/lib/hooks/useInstalledAgents', () => ({
  useInstalledAgents: () => ({
    hiddenIds: [],
    installedAgents: [{ id: 'pi', name: 'Pi' }],
  }),
}));
vi.mock('@/lib/api/account', () => ({ deleteAccount: vi.fn(), listAccounts: vi.fn() }));
vi.mock('@/lib/api/provider', () => ({ deleteProvider: vi.fn(), listProviders: vi.fn() }));
vi.mock('@/lib/api/adapter', () => ({
  createLocalToken: vi.fn(),
  deleteLocalToken: vi.fn(),
  listLocalTokens: vi.fn(),
  setLocalToken: testState.setLocalToken,
  setLocalTokenName: vi.fn(),
}));
vi.mock('@/pages/routes/shared/RoutesStartChecklist', () => ({ RoutesStartChecklist: () => null }));
vi.mock('@/pages/routes/shared/use-bridge-resources', () => ({
  useAdapterResources: () => ({
    profiles: [],
    bridgeStatuses: {},
    errors: { bridgeStatuses: {} },
    profileState: 'ready',
    loading: false,
    reload: vi.fn(),
  }),
}));
vi.mock('@/pages/routes/shared/use-route-pool-state', () => ({
  useRoutePoolState: () => ({ chatCompletionsShared: false, defaultPools: [], loading: false }),
}));
vi.mock('@/pages/routes/board/use-board-usage', () => ({
  useBoardUsageStats: () => ({ status: 'idle', rows: [] }),
}));
vi.mock('@/pages/routes/board/board-usage-model', () => ({
  boardUsageWindow: () => ({ since: '', days: 7 }),
}));
vi.mock('@/pages/routes/board/board-view-model', () => ({
  buildLocalGatewayControl: () => ({
    profileIds: [],
    running: false,
    hasEnrolledLogins: false,
    action: null,
    transitioning: false,
  }),
}));
vi.mock('@/pages/routes/RoutesPane', () => ({
  RoutesPane: ({ children }: { children?: ReactNode }) => children ?? null,
}));
vi.mock('./CreateTokenEndpointCards', () => ({ CreateTokenEndpointCards: () => null }));
vi.mock('./TokenDetailPanel', () => ({
  TokenDetailPanel: (props: Record<string, unknown>) => {
    testState.detailProps = props;
    return null;
  },
}));
vi.mock('./TokenList', () => ({
  TokenList: (props: Record<string, unknown>) => {
    testState.tokenListProps = props;
    return null;
  },
}));
vi.mock('./TokenImportToAgentButton', () => ({
  TokenImportToAgentButton: (props: Record<string, unknown>) => {
    testState.importButtonProps = props;
    return null;
  },
}));
vi.mock('./token-import-action', () => ({ applyImportedLogin: vi.fn() }));
vi.mock('./token-connection-matches', () => ({
  connectionMatchAgentNames: () => [],
  hashLocalToken: () => '',
  matchesConnectionEntryKeys: () => [],
}));
vi.mock('./tokens-model', () => ({
  buildCreateTokenEndpointCards: () => [],
  buildCreateTokenTargets: () => [],
  attachTokenUsage: (rows: readonly LocalTokenRow[]) => rows,
  buildLocalTokenRows: () => [ROW],
  defaultCreateTokenName: () => '',
  firstCreateTokenPoolId: () => '',
  generateLocalToken: () => 'ahb_generated',
  resolveCreateTokenPoolId: () => '',
  localTokenDeleteGate: () => ({ enabled: true, reason: null }),
  localTokenEditKeyGate: () => ({ enabled: true, reason: null }),
  maskLocalToken: (token: string) => token,
  tokenDisplayName: () => 'Codex entry',
  tokenTypeLabel: () => 'Responses',
}));

const { default: RoutesTokensPage } = await import('./index');

const ROW = {
  id: 'pool-codex',
  poolBacked: true,
  profileId: null,
  profileIds: [],
  name: 'Codex entry',
  primary: true,
  canDelete: true,
  kind: 'responses_codex',
  path: '/v1/responses',
  endpoint: 'http://127.0.0.1:17034/v1/responses',
  state: 'running',
  token: 'ahb_test',
  maskedToken: 'ahb_••••test',
  unavailable: false,
  targetAgentId: 'codex',
  lastPath: null,
  lastRequestAt: null,
  usageEligible: false,
  listedModels: ['gpt-5'],
} as LocalTokenRow;

const PI_RESPONSES_DRAFT: ConnectApiKeyDraft = {
  baseUrl: 'http://127.0.0.1:17034/v1',
  apiKey: 'ahb_test',
  model: 'gpt-5',
  piApi: 'openai-responses',
};

function renderPage(): string {
  testState.stateIndex = 0;
  testState.tokenListProps = null;
  testState.detailProps = null;
  testState.providerProps = null;
  testState.importButtonProps = null;
  return renderToStaticMarkup(createElement(RoutesTokensPage));
}

describe('RoutesTokensPage import wiring', () => {
  beforeEach(() => {
    testState.stateSlots = [];
    testState.stateIndex = 0;
    testState.inspect.target = null;
    testState.inspect.open.mockClear();
    testState.inspect.close.mockClear();
    testState.setLocalToken.mockReset();
  });

  it('passes the Pi Responses wire through the inline token-list import path', () => {
    renderPage();
    const onImport = testState.tokenListProps?.onImport as ((
      row: LocalTokenRow,
      agentId: 'pi',
      draft: ConnectApiKeyDraft,
    ) => void);

    onImport(ROW, 'pi', PI_RESPONSES_DRAFT);
    renderPage();

    expect(testState.providerProps).toMatchObject({
      agentId: 'pi',
      initialBaseUrl: 'http://127.0.0.1:17034/v1',
      initialApiKey: 'ahb_test',
      initialModel: 'gpt-5',
      initialPiApi: 'openai-responses',
    });
  });
});
