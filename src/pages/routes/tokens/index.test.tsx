import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ConnectApiKeyDraft } from '@/lib/connect-flow/connect-intent';
import type { Provider } from '@/lib/types';
import type { LocalTokenRow } from './tokens-model';

const testState = vi.hoisted(() => ({
  stateSlots: [] as unknown[],
  stateIndex: 0,
  tokenListProps: null as Record<string, unknown> | null,
  detailProps: null as Record<string, unknown> | null,
  providerProps: null as Record<string, unknown> | null,
  importButtonProps: null as Record<string, unknown> | null,
  listProviders: vi.fn(),
  applyImportedLogin: vi.fn(),
  reload: vi.fn(),
  toast: vi.fn(),
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
vi.mock('@/components/ui/toast', () => ({ useToast: () => ({ toast: testState.toast }) }));
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
vi.mock('@/lib/api/provider', () => ({ deleteProvider: vi.fn(), listProviders: testState.listProviders }));
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
    reload: testState.reload,
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
vi.mock('./token-import-action', () => ({ applyImportedLogin: testState.applyImportedLogin }));
vi.mock('./token-connection-matches', () => ({
  connectionMatchAgentNames: () => [],
  hashLocalToken: async () => '2cff9e711198f8d8764d34d67f92c838c9dfb1a9bbf7a94e7cf49552df3c4da9',
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

const REUSABLE_PI_PROVIDER: Provider = {
  id: 'pi-existing',
  agentId: 'pi',
  name: 'custom',
  preset: 'custom',
  configText: JSON.stringify({
    models: {
      providers: {
        custom: {
          baseUrl: 'http://127.0.0.1:17034/v1',
          api: 'openai-responses',
          models: [{ id: 'gpt-5' }],
        },
      },
    },
  }),
  configFormat: 'json',
  isCurrent: false,
  secretHash: '2cff9e711198f8d8764d34d67f92c838c9dfb1a9bbf7a94e7cf49552df3c4da9',
  updatedAt: '2026-10-02T02:00:00.000Z',
};

async function flushImport(): Promise<void> {
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
}

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
    testState.listProviders.mockReset();
    testState.listProviders.mockResolvedValue([]);
    testState.applyImportedLogin.mockReset();
    testState.applyImportedLogin.mockResolvedValue(undefined);
    testState.reload.mockReset();
    testState.toast.mockReset();
  });

  it('passes the Pi Responses wire through the inline token-list import path', async () => {
    renderPage();
    const onImport = testState.tokenListProps?.onImport as ((
      row: LocalTokenRow,
      agentId: 'pi',
      draft: ConnectApiKeyDraft,
    ) => void);

    onImport(ROW, 'pi', PI_RESPONSES_DRAFT);
    await flushImport();
    renderPage();

    expect(testState.providerProps).toMatchObject({
      agentId: 'pi',
      initialBaseUrl: 'http://127.0.0.1:17034/v1',
      initialApiKey: 'ahb_test',
      initialModel: 'gpt-5',
      initialPiApi: 'openai-responses',
    });
  });

  it('reuses one matching Pi provider for repeated imports without opening a new dialog', async () => {
    testState.listProviders.mockResolvedValue([REUSABLE_PI_PROVIDER]);
    renderPage();
    const onImport = testState.tokenListProps?.onImport as ((
      row: LocalTokenRow,
      agentId: 'pi',
      draft: ConnectApiKeyDraft,
    ) => void);

    onImport(ROW, 'pi', PI_RESPONSES_DRAFT);
    onImport(ROW, 'pi', PI_RESPONSES_DRAFT);
    await flushImport();

    expect(testState.listProviders).toHaveBeenCalledTimes(1);
    expect(testState.applyImportedLogin).toHaveBeenCalledTimes(1);
    expect(testState.applyImportedLogin).toHaveBeenCalledWith({
      agentId: 'pi',
      sourceKind: 'provider',
      sourceId: 'pi-existing',
      isCurrent: false,
    });
    expect(testState.providerProps).toBeNull();
    expect(testState.reload).toHaveBeenCalledTimes(1);
  });

  it('shows a safe error and does not expose lookup details when provider lookup fails', async () => {
    testState.listProviders.mockRejectedValue(new Error('raw-entry-key-secret'));
    renderPage();
    const onImport = testState.tokenListProps?.onImport as ((
      row: LocalTokenRow,
      agentId: 'pi',
      draft: ConnectApiKeyDraft,
    ) => void);

    onImport(ROW, 'pi', PI_RESPONSES_DRAFT);
    await flushImport();

    expect(testState.applyImportedLogin).not.toHaveBeenCalled();
    expect(testState.providerProps).toBeNull();
    expect(testState.toast).toHaveBeenCalledWith(expect.objectContaining({
      variant: 'danger',
    }));
    expect(testState.toast.mock.calls.flat()).not.toContain('raw-entry-key-secret');
  });

  it('does not let a deferred Pi lookup be overwritten by a Codex import', async () => {
    let resolveProviders!: (providers: Provider[]) => void;
    testState.listProviders.mockReturnValue(new Promise<Provider[]>((resolve) => {
      resolveProviders = resolve;
    }));
    renderPage();
    const onImport = testState.tokenListProps?.onImport as ((
      row: LocalTokenRow,
      agentId: 'pi' | 'codex',
      draft: ConnectApiKeyDraft,
    ) => void);

    onImport(ROW, 'pi', PI_RESPONSES_DRAFT);
    onImport(ROW, 'codex', {
      baseUrl: 'http://127.0.0.1:17034',
      apiKey: 'ahb_test',
      model: 'gpt-5',
    });
    resolveProviders([REUSABLE_PI_PROVIDER]);
    await flushImport();
    renderPage();

    expect(testState.applyImportedLogin).toHaveBeenCalledWith({
      agentId: 'pi',
      sourceKind: 'provider',
      sourceId: 'pi-existing',
      isCurrent: false,
    });
    expect(testState.providerProps).toBeNull();
  });

  it('keeps the existing add flow for non-Pi Agents', () => {
    renderPage();
    const onImport = testState.tokenListProps?.onImport as ((
      row: LocalTokenRow,
      agentId: 'codex',
      draft: ConnectApiKeyDraft,
    ) => void);

    onImport(ROW, 'codex', {
      baseUrl: 'http://127.0.0.1:17034',
      apiKey: 'ahb_test',
      model: 'gpt-5',
    });
    renderPage();

    expect(testState.listProviders).not.toHaveBeenCalled();
    expect(testState.providerProps).toMatchObject({
      agentId: 'codex',
      initialBaseUrl: 'http://127.0.0.1:17034',
      initialApiKey: 'ahb_test',
      initialModel: 'gpt-5',
    });
  });
});
