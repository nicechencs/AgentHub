import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import { CreateRouteDialog } from './CreateRouteDialog';
import { EditRouteDialog } from './EditRouteDialog';
import { ImportRouteDialog } from './ImportRouteDialog';
import { defaultCreateRouteName, endpointUrlFor, vendorById } from './create-route-flow';

vi.mock('@/components/ui/dialog', () => {
  const passthrough = ({ children }: { children?: ReactNode }) => children ?? null;
  return {
    Dialog: ({ open, children }: { open?: boolean; children?: ReactNode }) =>
      (open ? children : null),
    DialogContent: passthrough,
    DialogHeader: passthrough,
    DialogFooter: passthrough,
    DialogTitle: passthrough,
    DialogDescription: passthrough,
  };
});

vi.mock('react-router-dom', () => ({
  Link: ({ to, children }: { to: string; children?: ReactNode }) =>
    createElement('a', { href: to }, children),
}));

function renderCreate() {
  return renderToStaticMarkup(
    createElement(TooltipProvider, null, createElement(CreateRouteDialog, {
      open: true,
      onOpenChange: vi.fn(),
      onCreated: vi.fn(),
    })),
  );
}

describe('CreateRouteDialog', () => {
  it('opens as a right-side inspect panel with a form-linked submit', () => {
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(CreateRouteDialog, {
        open: true,
        asPanel: true,
        onOpenChange: vi.fn(),
        onCreated: vi.fn(),
      })),
    );
    expect(markup).toContain('data-side-inspect');
    expect(markup).toContain('form="create-route-form"');
    expect(markup).toContain('id="create-route-form"');
    expect(markup).toContain('type="submit"');
    expect(markup).toContain('收起');
    expect(markup).not.toContain('取消');
    expect(markup).not.toContain('/v1/messages');
    expect(markup).not.toContain('/v1/responses');
    expect(markup).not.toContain('协议桥');
  });

  it('disables 确认应用 until name+url+key+endpoint are filled', () => {
    const markup = renderCreate();
    expect(markup).toContain('确认应用');
    expect(markup).toContain('disabled');
    expect(markup).toContain('上游端点');
    expect(markup).toContain(endpointUrlFor('openrouter', 'claude', vendorById('openrouter').url));
    expect(markup).toContain('type="submit"');
  });

  it('keeps SecretInput and does not invent a second URL field', () => {
    const markup = renderCreate();
    expect(markup).toContain('type="password"');
    expect(markup).not.toContain('票');
    expect(markup).not.toContain('钱包');
    expect(markup).not.toContain('投影');
    expect(markup).not.toContain('协议桥');
  });
});

describe('ImportRouteDialog', () => {
  it('opens as a right-side inspect panel with a form-linked submit', () => {
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ImportRouteDialog, {
        open: true,
        asPanel: true,
        onOpenChange: vi.fn(),
        entries: [],
        onImported: vi.fn(),
      })),
    );
    expect(markup).toContain('data-side-inspect');
    expect(markup).toContain('form="import-route-form"');
    expect(markup).toContain('id="import-route-form"');
    expect(markup).toContain('type="submit"');
  });

  it('points empty state to 连接', () => {
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ImportRouteDialog, {
        open: true,
        onOpenChange: vi.fn(),
        entries: [],
        onImported: vi.fn(),
      })),
    );
    expect(markup).toContain('href="/connections"');
    expect(markup).toContain('连接');
    expect(markup).toContain('用这份登录');
    expect(markup).toContain('disabled');
  });

  it('renders each login as a row-sized radio label', () => {
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ImportRouteDialog, {
        open: true,
        onOpenChange: vi.fn(),
        entries: [{
          key: 'account:acc-1',
          source: 'account',
          kind: 'apikey',
          id: 'acc-1',
          agentId: 'claude',
          title: 'Work login',
          subtitle: '已配置',
          endpointMode: 'official',
          isCurrent: true,
          authStatus: 'valid',
          sortKey: '',
        }],
        onImported: vi.fn(),
      })),
    );
    expect(markup).toContain('type="radio"');
    expect(markup).toContain('Work login · Claude · 官方端点');
    expect(markup).toContain('cursor-pointer');
    expect(markup).toContain('确认应用');
    expect(markup).toContain('详情');
    expect(markup).toContain('aria-expanded="false"');
  });

  it('omits generated 本机路由 logins from import', () => {
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ImportRouteDialog, {
        open: true,
        onOpenChange: vi.fn(),
        entries: [
          {
            key: 'provider:p-1',
            source: 'provider',
            kind: 'apikey',
            id: 'p-1',
            agentId: 'claude',
            title: '本机路由',
            subtitle: '已配置 · 官方端点',
            isCurrent: true,
            authStatus: 'valid',
            sortKey: '',
            endpointMode: 'official',
          },
          {
            key: 'provider:p-2',
            source: 'provider',
            kind: 'apikey',
            id: 'p-2',
            agentId: 'codex',
            title: '本机路由',
            subtitle: '已配置 · 自定义端点',
            isCurrent: false,
            authStatus: 'valid',
            sortKey: '',
            endpointMode: 'custom',
          },
        ],
        onImported: vi.fn(),
      })),
    );
    expect(markup).not.toContain('本机路由 · Claude · 官方端点');
    expect(markup).not.toContain('本机路由 · Codex · 自定义端点');
    expect(markup).toContain('还没有可导入的登录');
    expect(markup).not.toContain('票');
    expect(markup).not.toContain('钱包');
    expect(markup).not.toContain('投影');
    expect(markup).not.toContain('协议桥');
    expect(markup).not.toContain('PKCE');
  });

  it('omits logins already attached to a local-bridge profile', () => {
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(ImportRouteDialog, {
        open: true,
        onOpenChange: vi.fn(),
        entries: [
          {
            key: 'account:acc-1',
            source: 'account',
            kind: 'apikey',
            id: 'acc-1',
            agentId: 'claude',
            title: 'Already routed',
            subtitle: '已配置',
            endpointMode: 'official',
            isCurrent: true,
            authStatus: 'valid',
            sortKey: '',
          },
          {
            key: 'account:acc-2',
            source: 'account',
            kind: 'apikey',
            id: 'acc-2',
            agentId: 'codex',
            title: 'Still free',
            subtitle: '已配置',
            endpointMode: 'official',
            isCurrent: false,
            authStatus: 'valid',
            sortKey: '',
          },
        ],
        profiles: [
          { id: 'prof-1', sourceKind: 'account', sourceId: 'acc-1', route: 'local_bridge' },
        ],
        onImported: vi.fn(),
      })),
    );
    expect(markup).not.toContain('Already routed');
    expect(markup).toContain('Still free · Codex · 官方端点');
    expect(markup).toContain('type="radio"');
  });
});

function editProfile(overrides: Partial<AdapterProfile> = {}): AdapterProfile {
  return {
    id: 'prof-1',
    name: 'OpenRouter',
    sourceKind: 'provider',
    sourceId: 'prov-1',
    targetAgentId: 'codex',
    route: 'local_bridge',
    mode: 'api',
    status: 'active',
    ruleId: 'openai-api-to-codex-v1',
    ruleVersion: 'v1',
    autoStart: true,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

function editEntry(overrides: Partial<ConnectionEntry> = {}): ConnectionEntry {
  return {
    key: 'provider:prov-1',
    source: 'provider',
    kind: 'apikey',
    id: 'prov-1',
    agentId: 'codex',
    title: 'OpenRouter',
    subtitle: '已配置',
    isCurrent: false,
    authStatus: 'valid',
    sortKey: '',
    provider: {
      id: 'prov-1',
      agentId: 'codex',
      name: 'OpenRouter',
      preset: 'openrouter',
      configFormat: 'json',
      configText: JSON.stringify({
        baseURL: 'https://openrouter.ai/api/v1',
        apiKey: 'stored-key',
        vendor: 'openrouter',
        endpoints: [{ target: 'codex', enabled: true, url: 'https://openrouter.ai/api/v1' }],
      }),
      isCurrent: false,
    },
    ...overrides,
  };
}

function renderEdit(profile: AdapterProfile, entries: readonly ConnectionEntry[]) {
  return renderToStaticMarkup(
    createElement(TooltipProvider, null, createElement(EditRouteDialog, {
      open: true,
      onOpenChange: vi.fn(),
      profile,
      entries,
      onSaved: vi.fn(),
      onRequestDelete: vi.fn(),
    })),
  );
}

describe('EditRouteDialog', () => {
  it('opens as a right-side inspect panel with a form-linked submit', () => {
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(EditRouteDialog, {
        open: true,
        asPanel: true,
        onOpenChange: vi.fn(),
        profile: editProfile(),
        entries: [editEntry()],
        onSaved: vi.fn(),
        onRequestDelete: vi.fn(),
      })),
    );
    expect(markup).toContain('data-side-inspect');
    expect(markup).toContain('form="edit-route-form"');
    expect(markup).toContain('id="edit-route-form"');
    expect(markup).toContain('type="submit"');
    expect(markup).toContain('收起');
    expect(markup).not.toContain('取消');
    expect(markup).not.toContain('/v1/messages');
    expect(markup).not.toContain('/v1/responses');
  });

  it('shows the unavailable copy for an account-sourced route', () => {
    const markup = renderEdit(
      editProfile({ sourceKind: 'account', sourceId: 'acc-1' }),
      [editEntry({ key: 'account:acc-1', source: 'account', id: 'acc-1', provider: undefined })],
    );
    expect(markup).toContain('这条路由的来源不是可编辑的 API 配置');
    expect(markup).not.toContain('上游端点');
    expect(markup).not.toContain('type="password"');
    expect(markup).toContain('删除路由');
    expect(markup).toContain('取消');
  });

  it('renders name, url, key, and 接到 fields plus 删除路由 for a JSON provider', () => {
    const markup = renderEdit(editProfile(), [editEntry()]);
    expect(markup).toContain('名称');
    expect(markup).toContain('地址');
    expect(markup).toContain('上游端点');
    expect(markup).toContain('type="password"');
    expect(markup).toContain('留空沿用现有密钥');
    expect(markup).toContain('type="checkbox"');
    expect(markup).toContain('type="submit"');
    expect(markup).toContain('保存');
    expect(markup).toContain('删除路由');
    expect(markup).not.toContain('stored-key');
    expect(markup).not.toContain('这条路由的来源不是可编辑的 API 配置');
  });

  it('seeds the stored name, url, and checked target on first render', () => {
    const markup = renderEdit(editProfile(), [editEntry()]);
    expect(markup).toContain('value="OpenRouter"');
    expect(markup).toContain('value="https://openrouter.ai/api/v1"');
    expect(markup).toContain('type="checkbox" class="mt-0.5" checked=""');
    expect(markup).not.toContain('value="stored-key"');
  });

  it('renders nothing without a profile', () => {
    expect(renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(EditRouteDialog, {
        open: true,
        onOpenChange: vi.fn(),
        profile: null,
        entries: [],
        onSaved: vi.fn(),
        onRequestDelete: vi.fn(),
      })),
    )).toBe('');
  });
});

describe('default create name', () => {
  it('uses the vendor label alone', () => {
    expect(defaultCreateRouteName('OpenRouter')).toBe('OpenRouter');
  });
});
