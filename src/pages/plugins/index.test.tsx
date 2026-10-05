import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  PluginEntry,
  PluginInventory,
  PluginMutationAction,
  PluginMutationOutcome,
} from '@/lib/backend/contracts/plugin-types';

const testState = vi.hoisted(() => ({
  stateSlots: [] as unknown[],
  stateIndex: 0,
  refSlots: [] as Array<{ current: unknown }>,
  refIndex: 0,
  installProps: null as Record<string, unknown> | null,
  uninstallProps: null as Record<string, unknown> | null,
  updateProps: null as Record<string, unknown> | null,
  detailProps: null as Record<string, unknown> | null,
  packProps: null as Record<string, unknown> | null,
  tabProps: null as Record<string, unknown> | null,
  refreshProps: null as Record<string, unknown> | null,
  buttons: [] as Array<Record<string, unknown>>,
  listInventory: vi.fn(),
  install: vi.fn(),
  uninstall: vi.fn(),
  enable: vi.fn(),
  disable: vi.fn(),
  refreshMarketplace: vi.fn(),
  update: vi.fn(),
  updatePi: vi.fn(),
  toast: vi.fn(),
  inspect: {
    target: null as PluginEntry | null,
    paneWidth: 420,
    expanded: false,
    open: vi.fn((target: PluginEntry) => { testState.inspect.target = target; }),
    close: vi.fn(() => { testState.inspect.target = null; }),
  },
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
            ? (next as (value: T) => T)(current)
            : next;
        },
      ] as const;
    },
    useRef: <T,>(initial: T) => {
      const index = testState.refIndex++;
      if (!(index in testState.refSlots)) testState.refSlots[index] = { current: initial };
      return testState.refSlots[index] as { current: T };
    },
    useMemo: <T,>(factory: () => T) => factory(),
    useCallback: <T extends (...args: never[]) => unknown>(callback: T) => callback,
    useEffect: () => undefined,
  };
});

vi.mock('lucide-react', () => ({ Plus: () => null, Puzzle: () => null }));
vi.mock('@/components/layout/AgentTabStrip', () => ({
  AgentTabStrip: (props: Record<string, unknown>) => { testState.tabProps = props; return null; },
}));
vi.mock('@/components/layout/PageHeader', () => ({ PageHeader: () => null }));
vi.mock('@/components/layout/SideSplit', () => ({
  WorkbenchSplitPage: ({ children, panel }: { children?: ReactNode; panel?: ReactNode }) => (
    createElement('div', null, children, panel)
  ),
}));
vi.mock('@/components/layout/use-side-split', () => ({ useSideSplit: () => testState.inspect }));
vi.mock('@/components/shared/EmptyState', () => ({ EmptyState: () => null }));
vi.mock('@/components/shared/ErrorState', () => ({ ErrorState: () => null }));
vi.mock('@/components/shared/Notice', () => ({ Notice: () => null }));
vi.mock('@/components/shared/PageRefreshButton', () => ({
  PageRefreshButton: (props: Record<string, unknown>) => { testState.refreshProps = props; return null; },
}));
vi.mock('@/components/shared/LanguageProvider', () => ({
  useI18n: () => ({ lang: 'zh', t: (key: string) => key }),
}));
vi.mock('@/components/ui/badge', () => ({ Badge: () => null }));
vi.mock('@/components/ui/button', () => ({
  Button: (props: Record<string, unknown>) => {
    testState.buttons.push(props);
    return props.children as ReactNode;
  },
}));
vi.mock('@/components/ui/skeleton', () => ({ ListSkeleton: () => null }));
vi.mock('@/components/ui/toast', () => ({ useToast: () => ({ toast: testState.toast }) }));
vi.mock('@/config/agents', () => ({ agentDisplayName: (id: string) => id }));
vi.mock('@/lib/agent-visibility', () => ({
  filterByPageVisibleAgent: (rows: PluginEntry[]) => rows,
}));
vi.mock('@/lib/hooks/useInstalledAgents', () => ({
  useInstalledAgents: () => ({
    hiddenIds: [],
    installedIds: ['claude', 'codex', 'grok', 'pi'],
    installedAgents: [
      { id: 'claude', name: 'Claude Code' },
      { id: 'codex', name: 'Codex' },
      { id: 'grok', name: 'Grok' },
      { id: 'pi', name: 'Pi' },
    ],
    loading: false,
  }),
}));
vi.mock('@/lib/api/plugins', () => ({
  listPluginInventory: testState.listInventory,
  installPlugin: testState.install,
  uninstallPlugin: testState.uninstall,
  enablePlugin: testState.enable,
  disablePlugin: testState.disable,
  refreshPluginMarketplace: testState.refreshMarketplace,
  updatePlugin: testState.update,
  updatePiPlugins: testState.updatePi,
}));
vi.mock('@/lib/api/skill', () => ({ openPathInFileManager: vi.fn() }));
vi.mock('./PluginDetailPanel', () => ({
  PluginDetailPanel: (props: Record<string, unknown>) => { testState.detailProps = props; return null; },
}));
vi.mock('./PluginInstallDialog', () => ({
  PluginInstallDialog: (props: Record<string, unknown>) => { testState.installProps = props; return null; },
}));
vi.mock('./PluginPackList', () => ({
  PluginPackList: (props: Record<string, unknown>) => { testState.packProps = props; return null; },
}));
vi.mock('./PluginUninstallDialog', () => ({
  PluginUninstallDialog: (props: Record<string, unknown>) => { testState.uninstallProps = props; return null; },
}));
vi.mock('./PluginUpdateDialog', () => ({
  PluginUpdateDialog: (props: Record<string, unknown>) => { testState.updateProps = props; return null; },
}));

const { default: PluginsPage } = await import('./index');

const CLAUDE_PLUGIN: PluginEntry = {
  id: 'claude:reviewer',
  agent: 'claude',
  name: 'reviewer',
  marketplace: 'official',
  version: '1.0.0',
  scope: 'user',
  enabled: true,
  source: 'cli',
  components: [],
};

const GROK_PLUGIN: PluginEntry = {
  id: 'grok:reviewer#~/.grok/plugins/reviewer',
  agent: 'grok',
  name: 'reviewer',
  marketplace: 'xAI Official',
  installSource: '~/src/reviewer',
  version: '1.0.0',
  scope: 'user',
  enabled: true,
  source: 'cli',
  components: [],
};

const PI_PLUGIN: PluginEntry = {
  id: 'pi:npm:@agenthub/pi-pack',
  agent: 'pi',
  name: '@agenthub/pi-pack',
  installSource: 'npm:@agenthub/pi-pack@1.2.3',
  source: 'live',
  components: [],
};

const INVENTORY: PluginInventory = {
  agents: ['claude', 'codex', 'grok', 'pi'].map((agent) => ({
    agent: agent as 'claude' | 'codex' | 'grok' | 'pi',
    support: 'listed',
    pluginCount: agent === 'claude' || agent === 'pi' ? 1 : 0,
  })),
  plugins: [CLAUDE_PLUGIN, PI_PLUGIN],
};

function mutationOutcome(
  action: PluginMutationAction,
  agent: PluginMutationOutcome['agent'] = 'claude',
  inventory: PluginInventory = INVENTORY,
): PluginMutationOutcome {
  return {
    status: 'confirmed',
    action,
    agent,
    inventory,
    reinventory: { scope: 'target', scannedPluginIds: [] },
  };
}

function renderPage(): void {
  testState.stateIndex = 0;
  testState.refIndex = 0;
  testState.installProps = null;
  testState.uninstallProps = null;
  testState.updateProps = null;
  testState.detailProps = null;
  testState.packProps = null;
  testState.tabProps = null;
  testState.refreshProps = null;
  testState.buttons = [];
  renderToStaticMarkup(createElement(PluginsPage));
}

async function settle(): Promise<void> {
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
}

async function loadInventory(): Promise<void> {
  renderPage();
  (testState.refreshProps?.onClick as () => void)();
  await settle();
  renderPage();
}

function actionButton(label: string): Record<string, unknown> {
  const includesLabel = (value: unknown): boolean => {
    if (value === label) return true;
    if (Array.isArray(value)) return value.some(includesLabel);
    if (value && typeof value === 'object' && 'props' in value) {
      return includesLabel((value as { props?: { children?: unknown } }).props?.children);
    }
    return false;
  };
  const button = testState.buttons.find((row) => includesLabel(row.children));
  if (!button) throw new Error(`button not found: ${label}`);
  return button;
}

describe('PluginsPage mutation wiring', () => {
  beforeEach(() => {
    testState.stateSlots = [];
    testState.refSlots = [];
    testState.inspect.target = null;
    testState.inspect.open.mockClear();
    testState.inspect.close.mockClear();
    testState.listInventory.mockReset().mockResolvedValue(INVENTORY);
    for (const mock of [
      testState.install,
      testState.uninstall,
      testState.enable,
      testState.disable,
      testState.refreshMarketplace,
      testState.update,
      testState.updatePi,
    ]) mock.mockReset();
    testState.install.mockResolvedValue(mutationOutcome('install', 'grok'));
    testState.uninstall.mockResolvedValue(mutationOutcome('uninstall'));
    testState.enable.mockResolvedValue(mutationOutcome('enable'));
    testState.disable.mockResolvedValue(mutationOutcome('disable'));
    testState.refreshMarketplace.mockResolvedValue(mutationOutcome('marketplaceRefresh', 'codex'));
    testState.update.mockResolvedValue(mutationOutcome('update'));
    testState.updatePi.mockResolvedValue(mutationOutcome('piUpdate', 'pi'));
    testState.toast.mockReset();
  });

  it('installs once, reloads inventory, and closes only after success', async () => {
    await loadInventory();
    (actionButton('plugins.install.button').onClick as () => void)();
    renderPage();
    expect(testState.installProps?.open).toBe(true);
    let resolveInstall!: (outcome: PluginMutationOutcome) => void;
    const pending = new Promise<PluginMutationOutcome>((resolve) => { resolveInstall = resolve; });
    testState.install.mockReturnValueOnce(pending);
    const install = testState.installProps?.onInstall as (
      agent: 'grok', source: string, confirmed: boolean,
    ) => Promise<void>;

    const first = install('grok', 'reviewer@official', true);
    const duplicate = install('grok', 'ignored@official', true);
    expect(testState.install).toHaveBeenCalledTimes(1);
    expect(testState.install).toHaveBeenCalledWith('grok', 'reviewer@official', { confirmed: true });
    renderPage();
    expect(testState.installProps?.open).toBe(true);
    expect(testState.installProps?.busy).toBe(true);
    expect(testState.refreshProps?.disabled).toBe(true);

    resolveInstall(mutationOutcome('install', 'grok'));
    await Promise.all([first, duplicate]);
    renderPage();
    expect(testState.listInventory).toHaveBeenCalledTimes(1);
    expect(testState.installProps?.open).toBe(false);
    expect(testState.toast).toHaveBeenCalledWith({ title: 'plugins.install.ok', variant: 'success' });
  });

  it('keeps the install dialog error and skips reload when the command fails', async () => {
    await loadInventory();
    (actionButton('plugins.install.button').onClick as () => void)();
    renderPage();
    testState.install.mockRejectedValueOnce(new Error('fixture install failed'));
    await (testState.installProps?.onInstall as (
      agent: 'claude', source: string, confirmed: boolean,
    ) => Promise<void>)('claude', 'broken@official', true);
    renderPage();

    expect(testState.listInventory).toHaveBeenCalledTimes(1);
    expect(testState.installProps?.open).toBe(true);
    expect(testState.installProps?.error).toEqual(new Error('fixture install failed'));
  });

  it('keeps the install dialog open and never toasts success for an unconfirmed identity', async () => {
    await loadInventory();
    (actionButton('plugins.install.button').onClick as () => void)();
    renderPage();
    testState.install.mockResolvedValueOnce({
      ...mutationOutcome('install', 'grok'),
      status: 'unconfirmed',
      reason: 'ambiguousTarget',
    });

    await (testState.installProps?.onInstall as (
      agent: 'grok', source: string, confirmed: boolean,
    ) => Promise<void>)('grok', 'reviewer@official', true);
    renderPage();

    expect(testState.installProps?.open).toBe(true);
    expect(testState.installProps?.error).toBe('plugins.outcome.unconfirmed');
    expect(testState.toast).not.toHaveBeenCalledWith({ title: 'plugins.install.ok', variant: 'success' });
    expect(testState.listInventory).toHaveBeenCalledTimes(1);
  });

  it('does not toast toggle success when Grok has same-name local candidates', async () => {
    const duplicate: PluginEntry = {
      ...GROK_PLUGIN,
      id: 'grok:reviewer#~/.grok/plugins/reviewer-local',
      installSource: '~/src/reviewer-local',
    };
    const ambiguousInventory: PluginInventory = {
      ...INVENTORY,
      plugins: [GROK_PLUGIN, duplicate],
    };
    await loadInventory();
    testState.inspect.target = GROK_PLUGIN;
    testState.disable.mockResolvedValueOnce({
      ...mutationOutcome('disable', 'grok', ambiguousInventory),
      status: 'unconfirmed',
      reason: 'ambiguousTarget',
      reinventory: {
        scope: 'target',
        scannedPluginIds: [GROK_PLUGIN.id, duplicate.id],
      },
    });
    renderPage();

    await (testState.detailProps?.onToggle as (
      plugin: PluginEntry, enabled: boolean,
    ) => Promise<void>)(GROK_PLUGIN, false);

    expect(testState.disable).toHaveBeenCalledWith('grok', 'reviewer', 'xAI Official');
    expect(testState.toast).toHaveBeenCalledWith({
      title: 'plugins.outcome.unconfirmed',
      variant: 'danger',
    });
    expect(testState.toast).not.toHaveBeenCalledWith({
      title: 'plugins.actions.disabled',
      variant: 'success',
    });
  });

  it('routes toggle, update, and uninstall with the complete plugin identity', async () => {
    await loadInventory();
    testState.inspect.target = CLAUDE_PLUGIN;
    renderPage();

    await (testState.detailProps?.onToggle as (
      plugin: PluginEntry, enabled: boolean,
    ) => Promise<void>)(CLAUDE_PLUGIN, false);
    expect(testState.disable).toHaveBeenCalledWith('claude', 'reviewer', 'official');

    renderPage();
    await (testState.detailProps?.onToggle as (
      plugin: PluginEntry, enabled: boolean,
    ) => Promise<void>)(CLAUDE_PLUGIN, true);
    expect(testState.enable).toHaveBeenCalledWith('claude', 'reviewer', 'official');

    renderPage();
    (testState.detailProps?.onUpdate as (plugin: PluginEntry) => void)(CLAUDE_PLUGIN);
    renderPage();
    await (testState.updateProps?.onConfirm as (target: unknown) => Promise<void>)(
      testState.updateProps?.target,
    );
    expect(testState.update).toHaveBeenCalledWith(
      'claude', 'reviewer', 'official', 'user', { confirmed: true },
    );

    testState.inspect.target = CLAUDE_PLUGIN;
    renderPage();
    (testState.detailProps?.onUninstall as (plugin: PluginEntry) => void)(CLAUDE_PLUGIN);
    renderPage();
    await (testState.uninstallProps?.onUninstall as (
      plugin: PluginEntry, keepData: boolean,
    ) => Promise<void>)(CLAUDE_PLUGIN, false);
    expect(testState.uninstall).toHaveBeenCalledWith(
      'claude', 'reviewer', 'official', undefined, { keepData: false },
    );
    expect(testState.inspect.close).toHaveBeenCalled();
    expect(testState.listInventory).toHaveBeenCalledTimes(1);
  });

  it('refreshes a selected marketplace and runs the Pi all-extension update', async () => {
    await loadInventory();
    (testState.tabProps?.onChange as (agent: string) => void)('codex');
    renderPage();
    (actionButton('plugins.marketplace.button').onClick as () => void)();
    await settle();
    expect(testState.refreshMarketplace).toHaveBeenCalledWith('codex');

    (testState.tabProps?.onChange as (agent: string) => void)('pi');
    renderPage();
    (actionButton('plugins.update.piButton').onClick as () => void)();
    renderPage();
    await (testState.updateProps?.onConfirm as (target: unknown) => Promise<void>)(
      testState.updateProps?.target,
    );
    expect(testState.updatePi).toHaveBeenCalledWith({ confirmed: true });
    expect(testState.listInventory).toHaveBeenCalledTimes(1);
  });
});
