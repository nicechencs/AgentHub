import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { LocalTokenRow } from './tokens-model';
import type { TokenImportAgentRef } from './token-import-model';

const testState = vi.hoisted(() => ({
  stateSlots: [] as unknown[],
  stateIndex: 0,
  refSlots: [] as Array<{ current: unknown }>,
  refIndex: 0,
  effectCallbacks: [] as Array<() => void | (() => void)>,
  effectCleanups: [] as Array<(() => void) | undefined>,
  effectIndex: 0,
  menuItems: [] as Array<() => void>,
  buttons: [] as Array<{
    onClick?: () => void;
    disabled?: boolean;
  }>,
  dialogOnOpenChange: undefined as ((open: boolean) => void) | undefined,
  getLocalGatewayStatus: vi.fn(),
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
          testState.stateSlots[index] = typeof next === 'function'
            ? (next as (current: T) => T)(testState.stateSlots[index] as T)
            : next;
        },
      ] as const;
    },
    useRef: <T,>(initial: T) => {
      const index = testState.refIndex++;
      if (!(index in testState.refSlots)) testState.refSlots[index] = { current: initial };
      return testState.refSlots[index] as { current: T };
    },
    useEffect: (effect: () => void | (() => void)) => {
      const index = testState.effectIndex++;
      if (!(index in testState.effectCallbacks)) {
        testState.effectCallbacks[index] = effect;
        testState.effectCleanups[index] = effect() || undefined;
      }
    },
  };
});

vi.mock('@/lib/api/adapter', () => ({
  getLocalGatewayStatus: testState.getLocalGatewayStatus,
}));

vi.mock('@/components/shared/LanguageProvider', () => ({
  useI18n: () => ({
    t: (key: string) => ({
      'routes.tokens.importToAgent': '填入某个 Agent',
      'routes.tokens.importConfirmTitle': '填入 Codex',
      'routes.tokens.importConfirmDescription': '确定填入这把入口 Key？',
      'routes.tokens.importCheckingGateway': '检查本机转发中…',
      'routes.tokens.importGatewayClosedTitle': '本机转发已关闭',
      'routes.tokens.importGatewayClosedDescription': '本机转发已关闭，请先开启后再导入。',
      'routes.tokens.importGatewayRestartingTitle': '本机转发正在恢复',
      'routes.tokens.importGatewayRestartingDescription': '本机转发正在启动或恢复，请稍后再导入。',
      'routes.tokens.importGatewayStatusFailedTitle': '无法确认本机转发状态',
      'routes.tokens.importGatewayStatusFailedDescription': '暂时无法确认本机转发状态，请稍后重试。',
      'routes.tokens.importOpenRoutes': '去路由总览开启',
      'routes.tokens.importNeedKey': '先有入口 Key 才能填入',
      'common.cancel': '取消',
    }[key] ?? key),
  }),
}));

vi.mock('@/components/shared/AgentDot', () => ({ AgentDot: () => null }));
vi.mock('@/components/ui/toast', () => ({ useToast: () => ({ toast: vi.fn() }) }));
vi.mock('@/components/ui/tooltip', () => ({ Hint: ({ children }: { children: unknown }) => children }));
vi.mock('@/components/ui/button', async () => {
  const React = await vi.importActual<typeof import('react')>('react');
  return {
    Button: ({ onClick, disabled, children }: { onClick?: () => void; disabled?: boolean; children?: ReactNode }) => {
      testState.buttons.push({ onClick, disabled });
      return React.createElement('button', { disabled, onClick }, children);
    },
  };
});
vi.mock('@/components/ui/dialog', async () => {
  const React = await vi.importActual<typeof import('react')>('react');
  const passthrough = ({ children }: { children?: unknown }) => children ?? null;
  return {
    Dialog: ({ open, children, onOpenChange }: {
      open?: boolean;
      children?: unknown;
      onOpenChange?: (open: boolean) => void;
    }) => {
      testState.dialogOnOpenChange = onOpenChange;
      return open ? children : null;
    },
    DialogContent: passthrough,
    DialogHeader: passthrough,
    DialogFooter: passthrough,
    DialogTitle: passthrough,
    DialogDescription: passthrough,
    __react: React,
  };
});
vi.mock('@/components/ui/dropdown-menu', async () => {
  const React = await vi.importActual<typeof import('react')>('react');
  const passthrough = ({ children }: { children?: unknown }) => children ?? null;
  return {
    DropdownMenu: passthrough,
    DropdownMenuTrigger: passthrough,
    DropdownMenuContent: passthrough,
    DropdownMenuItem: ({ onSelect, children, disabled }: {
      onSelect?: () => void;
      children?: ReactNode;
      disabled?: boolean;
    }) => {
      if (!disabled && onSelect) testState.menuItems.push(onSelect);
      return React.createElement('div', { onClick: onSelect }, children);
    },
  };
});
vi.mock('react-router-dom', async () => {
  const React = await vi.importActual<typeof import('react')>('react');
  return {
    Link: ({ to, children }: { to: string; children?: ReactNode }) => (
      React.createElement('a', { href: to }, children)
    ),
  };
});

const { TokenImportToAgentButton } = await import('./TokenImportToAgentButton');

const ROW: LocalTokenRow = {
  id: 'pool-codex',
  poolBacked: true,
  profileId: 'profile-codex',
  profileIds: ['profile-codex'],
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
};

const AGENTS: readonly TokenImportAgentRef[] = [{ id: 'codex', name: 'Codex' }];

function renderButton(onImport = vi.fn()): { markup: string; onImport: ReturnType<typeof vi.fn> } {
  testState.stateIndex = 0;
  testState.refIndex = 0;
  testState.effectIndex = 0;
  testState.menuItems = [];
  testState.buttons = [];
  const markup = renderToStaticMarkup(createElement(TokenImportToAgentButton, {
    row: ROW,
    installedAgents: AGENTS,
    onImport,
  }));
  return { markup, onImport };
}

function openConfirmDialog(onImport = vi.fn()) {
  renderButton(onImport);
  testState.menuItems[0]?.();
  return renderButton(onImport);
}

async function clickConfirm(onImport: ReturnType<typeof vi.fn>) {
  openConfirmDialog(onImport);
  testState.buttons.at(-1)?.onClick?.();
  await Promise.resolve();
  await Promise.resolve();
}

describe('TokenImportToAgentButton local forwarding guard', () => {
  beforeEach(() => {
    testState.stateSlots = [];
    testState.refSlots = [];
    testState.effectCallbacks = [];
    testState.effectCleanups = [];
    testState.getLocalGatewayStatus.mockReset();
    testState.dialogOnOpenChange = undefined;
  });

  it('keeps the dialog open and links to the Routes board when forwarding is closed', async () => {
    testState.getLocalGatewayStatus.mockResolvedValue({ running: false, restarting: false });
    const onImport = vi.fn();

    await clickConfirm(onImport);
    const { markup } = renderButton(onImport);

    expect(onImport).not.toHaveBeenCalled();
    expect(markup).toContain('本机转发已关闭，请先开启后再导入。');
    expect(markup).toContain('href="/routes/board"');
    expect(markup).toContain('去路由总览开启');
  });

  it('imports after the status check reports a running gateway', async () => {
    testState.getLocalGatewayStatus.mockResolvedValue({ running: true, restarting: false });
    const onImport = vi.fn();

    await clickConfirm(onImport);

    expect(onImport).toHaveBeenCalledOnce();
  });

  it('keeps the dialog open without importing when status cannot be read', async () => {
    testState.getLocalGatewayStatus.mockRejectedValue(new Error('backend unavailable'));
    const onImport = vi.fn();

    await clickConfirm(onImport);
    const { markup } = renderButton(onImport);

    expect(onImport).not.toHaveBeenCalled();
    expect(markup).toContain('无法确认本机转发状态');
    expect(markup).toContain('暂时无法确认本机转发状态，请稍后重试。');
  });

  it('shows a restarting message instead of treating recovery as stopped', async () => {
    testState.getLocalGatewayStatus.mockResolvedValue({ running: false, restarting: true });
    const onImport = vi.fn();

    await clickConfirm(onImport);
    const { markup } = renderButton(onImport);

    expect(onImport).not.toHaveBeenCalled();
    expect(markup).toContain('本机转发正在恢复');
    expect(markup).toContain('本机转发正在启动或恢复，请稍后再导入。');
  });

  it('does not submit a second status check while the first one is pending', async () => {
    let resolveStatus!: (status: { running: boolean; restarting: boolean }) => void;
    testState.getLocalGatewayStatus.mockReturnValue(new Promise((resolve) => {
      resolveStatus = resolve;
    }));
    const onImport = vi.fn();
    openConfirmDialog(onImport);
    const confirm = testState.buttons.at(-1)?.onClick;

    confirm?.();
    confirm?.();
    expect(testState.getLocalGatewayStatus).toHaveBeenCalledOnce();

    resolveStatus({ running: true, restarting: false });
    await Promise.resolve();
    await Promise.resolve();
    expect(onImport).toHaveBeenCalledOnce();
  });

  it('cancels a deferred check when the confirmation closes before it resolves', async () => {
    let resolveStatus!: (status: { running: boolean; restarting: boolean }) => void;
    testState.getLocalGatewayStatus.mockReturnValue(new Promise((resolve) => {
      resolveStatus = resolve;
    }));
    const onImport = vi.fn();
    openConfirmDialog(onImport);
    testState.buttons.at(-1)?.onClick?.();

    testState.dialogOnOpenChange?.(false);
    resolveStatus({ running: true, restarting: false });
    await Promise.resolve();
    await Promise.resolve();

    expect(onImport).not.toHaveBeenCalled();
  });

  it('ignores a deferred result after the button unmounts', async () => {
    let resolveStatus!: (status: { running: boolean; restarting: boolean }) => void;
    testState.getLocalGatewayStatus.mockReturnValue(new Promise((resolve) => {
      resolveStatus = resolve;
    }));
    const onImport = vi.fn();
    openConfirmDialog(onImport);
    testState.buttons.at(-1)?.onClick?.();

    testState.effectCleanups[0]?.();
    resolveStatus({ running: true, restarting: false });
    await Promise.resolve();
    await Promise.resolve();

    expect(onImport).not.toHaveBeenCalled();
  });

  it('restores the mounted guard when StrictMode replays the effect', async () => {
    testState.getLocalGatewayStatus.mockResolvedValue({ running: true, restarting: false });
    const onImport = vi.fn();
    openConfirmDialog(onImport);

    testState.effectCleanups[0]?.();
    testState.effectCleanups[0] = testState.effectCallbacks[0]?.() || undefined;
    testState.buttons.at(-1)?.onClick?.();
    await Promise.resolve();
    await Promise.resolve();

    expect(onImport).toHaveBeenCalledOnce();
  });
});
