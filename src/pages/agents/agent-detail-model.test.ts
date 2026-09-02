import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { AGENT_MAP } from '@/config/agents';
import { translate } from '@/lib/i18n';
import { zh } from '@/lib/i18n/locales/zh';
import { TooltipProvider } from '@/components/ui/tooltip';
import { extraCopyKindLabel, extraCopyKindLabelKey } from './agent-card-model';
import { AgentDetailPanel } from './AgentDetailPanel';
import {
  agentConversationEndpoints,
  agentConversationSurface,
  agentConversationSurfaces,
  catalogChannelCommand,
  catalogChannelLabel,
  copyableChannelCommand,
  displayAgentConfigDir,
  formatAgentConversationEndpoints,
  installChannelKindLabel,
  installLocationSourceLabel,
  isNpmPackageCatalogLabel,
  isRawInstallChannelLabel,
  missingCatalogChannels,
} from './agent-detail-model';
import type { AgentStatus } from '@/lib/types';

function renderPanel(agent: AgentStatus): string {
  return renderToStaticMarkup(
    createElement(
      TooltipProvider,
      null,
      createElement(AgentDetailPanel, {
        agent,
        width: 360,
        onClose: () => undefined,
        onChanged: () => undefined,
      }) as ReactNode,
    ),
  );
}

const tZh = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) =>
  translate('zh', key, params);
const tEn = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) =>
  translate('en', key, params);

function installed(agentId: string, channel = 'native'): AgentStatus {
  return {
    agentId,
    installed: true,
    version: '2.1.235',
    channel,
    binPath: `/home/box/.local/bin/${agentId}`,
    authStatus: 'none',
    authLabel: '未配置',
    running: false,
  };
}

describe('agent conversation surfaces', () => {
  it('maps catalog agents onto the conversation paths they actually support', () => {
    expect(agentConversationSurface('claude')).toBe('messages');
    expect(agentConversationSurface('codex')).toBe('responses');
    expect(agentConversationSurface('grok')).toBe('responses');
    expect(agentConversationSurface('kimi')).toBe('messages');
    expect(agentConversationSurface('dsh')).toBe('messages');
    expect(agentConversationSurface('workbuddy')).toBe('chat_completions');
    expect(agentConversationEndpoints('claude')).toEqual([
      {
        id: 'messages',
        path: '/v1/messages',
        brandAgentId: 'claude',
      },
    ]);
    expect(agentConversationEndpoints('codex')[0]?.path).toBe('/v1/responses');
    expect(agentConversationEndpoints('grok')[0]).toMatchObject({
      path: '/v1/responses',
      brandAgentId: 'grok',
    });
    expect(agentConversationEndpoints('kimi').map((row) => row.path)).toEqual([
      '/v1/messages',
      '/v1/responses',
      '/v1/chat/completions',
    ]);
    expect(agentConversationEndpoints('workbuddy')[0]).toMatchObject({
      id: 'chat_completions',
      path: '/v1/chat/completions',
      brandAgentId: 'codex',
    });
  });

  it('lists DeepSeek official Messages, Responses, and Chat Completions', () => {
    expect(agentConversationSurfaces('dsh')).toEqual([
      'messages',
      'responses',
      'chat_completions',
    ]);
    expect(agentConversationEndpoints('dsh').map((row) => [row.path, row.brandAgentId])).toEqual([
      ['/v1/messages', 'claude'],
      ['/v1/responses', 'codex'],
      ['/v1/chat/completions', 'codex'],
    ]);
    expect(formatAgentConversationEndpoints('dsh', tZh)).toBe(
      ['/v1/messages', '/v1/responses', '/v1/chat/completions'].join('\n'),
    );
  });

  it('lists ZCode, Kimi Code, and Pi on Messages, Responses, and Chat Completions', () => {
    for (const agentId of ['zcode', 'kimi', 'pi'] as const) {
      expect(agentConversationSurfaces(agentId)).toEqual([
        'messages',
        'responses',
        'chat_completions',
      ]);
      expect(agentConversationEndpoints(agentId).map((row) => [row.id, row.brandAgentId])).toEqual([
        ['messages', 'claude'],
        ['responses', 'codex'],
        ['chat_completions', 'codex'],
      ]);
      expect(formatAgentConversationEndpoints(agentId, tZh)).toBe(
        ['/v1/messages', '/v1/responses', '/v1/chat/completions'].join('\n'),
      );
    }
  });

  it('does not invent a public HTTP surface for Cursor Agent', () => {
    expect(agentConversationSurface('cursor')).toBeNull();
    expect(agentConversationEndpoints('cursor')).toEqual([]);
    expect(formatAgentConversationEndpoints('cursor', tZh)).toBe('随当前登录而定');
    expect(formatAgentConversationEndpoints('cursor', tEn)).toBe('Depends on the current login');
  });

  it('lists only the conversation path, without a second Agent name', () => {
    expect(formatAgentConversationEndpoints('claude', tZh)).toBe('/v1/messages');
    expect(formatAgentConversationEndpoints('claude', tEn)).toBe('/v1/messages');
    expect(formatAgentConversationEndpoints('grok', tZh)).toBe('/v1/responses');
    expect(formatAgentConversationEndpoints('kimi', tZh)).toBe(
      ['/v1/messages', '/v1/responses', '/v1/chat/completions'].join('\n'),
    );
    expect(formatAgentConversationEndpoints('workbuddy', tEn)).toBe('/v1/chat/completions');
    expect(formatAgentConversationEndpoints('zcode', tZh)).toBe(
      ['/v1/messages', '/v1/responses', '/v1/chat/completions'].join('\n'),
    );
  });
});

describe('install channel labels', () => {
  it('never treats raw native as product copy', () => {
    expect(isRawInstallChannelLabel('native', 'native')).toBe(true);
    expect(isRawInstallChannelLabel('native', 'native 官方脚本')).toBe(true);
    expect(isRawInstallChannelLabel('native', '官网 Setup（打开安装页）')).toBe(false);
    expect(isRawInstallChannelLabel('npm', 'npm')).toBe(true);
    expect(isRawInstallChannelLabel('npm', 'npm @anthropic-ai/claude-code')).toBe(false);
    expect(isNpmPackageCatalogLabel('npm', 'npm @openai/codex')).toBe(true);
    expect(isNpmPackageCatalogLabel('native', '官网 Setup（打开安装页）')).toBe(false);
  });

  it('uses dest human kind on 渠道 and keeps package ids on 安装位置', () => {
    expect(catalogChannelLabel('claude', 'native')).toBeUndefined();
    expect(catalogChannelLabel('workbuddy', 'native')).toBe('官网 Setup（打开安装页）');
    expect(catalogChannelLabel('claude', 'npm')).toBe('npm @anthropic-ai/claude-code');
    expect(installChannelKindLabel('claude', 'native', tZh)).toBe('官方脚本');
    expect(installChannelKindLabel('claude', 'native', tEn)).toBe('Official script');
    expect(installChannelKindLabel('codex', 'npm', tZh)).toBe('npm 包');
    expect(installChannelKindLabel('pi', 'npm', tZh)).toBe('npm 包');
    expect(installChannelKindLabel('dsh', 'npm', tZh)).toBe('npm 包');
    expect(installChannelKindLabel('codex', 'npm', tEn)).toBe('npm package');
    expect(installLocationSourceLabel('codex', 'npm', tZh)).toBe('npm @openai/codex');
    expect(installLocationSourceLabel('pi', 'npm', tZh)).toBe('npm @earendil-works/pi-coding-agent');
    expect(installLocationSourceLabel('dsh', 'npm', tZh)).toBe('npm @deepseek-ai/dsh');
    expect(installChannelKindLabel('workbuddy', 'native', tZh)).toBe('官网 Setup（打开安装页）');
    expect(installChannelKindLabel('claude', 'ide', tZh)).toBe('IDE 插件');
    expect(installChannelKindLabel('claude', 'desktop', tZh)).toBe('桌面应用');
    expect(installChannelKindLabel('claude', undefined, tZh)).toBeUndefined();
    expect(AGENT_MAP.claude?.installChannels.some((row) => row.id === 'native')).toBe(true);
    expect(extraCopyKindLabelKey('native')).toBe('agents.card.channelOfficial');
    expect(extraCopyKindLabel('native', tZh)).toBe('官方脚本');
    expect(extraCopyKindLabel('npm', tZh)).toBe('npm 包');
    expect(zh.agents.card.channelNpm).toBe('npm 包');
    expect(catalogChannelCommand('claude', 'native')).toMatch(/install\.(ps1|sh)/);
    expect(catalogChannelCommand('dsh', 'npm')).toBe('npm i -g @deepseek-ai/dsh');
    expect(copyableChannelCommand('claude', 'native', tZh)).toBe(catalogChannelCommand('claude', 'native'));
    expect(copyableChannelCommand('dsh', 'npm', tZh)).toBe('npm i -g @deepseek-ai/dsh');
    expect(copyableChannelCommand('workbuddy', 'native', tZh)).toBeUndefined();
    expect(copyableChannelCommand('claude', 'ide', tZh)).toBeUndefined();
  });
});

describe('missing catalog channels', () => {
  it('lists Codex npm when that copy is not on disk', () => {
    const missing = missingCatalogChannels({
      agentId: 'codex',
      installed: false,
    });
    expect(missing.some((row) => row.id === 'npm')).toBe(true);
    expect(missing.find((row) => row.id === 'npm')?.command).toBe('npm i -g @openai/codex');
  });

  it('omits Codex npm after that copy is installed, even if leftover remains', () => {
    expect(
      missingCatalogChannels(installed('codex', 'npm')).every((row) => row.id !== 'npm'),
    ).toBe(true);
    expect(
      missingCatalogChannels({
        ...installed('codex', 'native'),
        extraCopies: [],
      }).some((row) => row.id === 'npm'),
    ).toBe(true);
  });
});

describe('config directory display', () => {
  it('shows dest known paths and omits heading-only or home-only fallbacks', () => {
    expect(displayAgentConfigDir('claude')).toBe('~/.claude');
    expect(displayAgentConfigDir('codex')).toBe('~/.codex');
    expect(displayAgentConfigDir('grok')).toBe('~/.grok');
    expect(displayAgentConfigDir('kimi')).toBe('~/.kimi-code');
    expect(displayAgentConfigDir('dsh')).toBe('~/.dsh');
    expect(displayAgentConfigDir('pi')).toBe('~/.pi/agent');
    expect(displayAgentConfigDir('claude', '/home/box/.claude')).toBe('/home/box/.claude');
    expect(displayAgentConfigDir('unknown-demo')).toBeNull();
    expect(displayAgentConfigDir('unknown-demo', '~')).toBeNull();
  });
});

describe('AgentDetailPanel markup', () => {
  it('renders endpoint types, a human channel, and a real config path for Claude', () => {
    const html = renderPanel(installed('claude'));
    expect(html).toContain('端点类型');
    expect(html).toContain('/v1/messages');
    expect(html).not.toContain('Claude 对话');
    expect(html).toContain('var(--agent-claude)');
    expect(html).toContain('>官方脚本</button>');
    expect(html).not.toMatch(/>native</);
    expect(html).toContain('~/.claude');
    expect(html).not.toMatch(/配置目录<\/span>/);
  });

  it('colors each Pi surface with its brand Agent and lists all three paths', () => {
    const html = renderPanel(installed('pi', 'npm'));
    expect(html).toContain('端点类型');
    expect(html).toContain('/v1/messages');
    expect(html).toContain('/v1/responses');
    expect(html).toContain('/v1/chat/completions');
    expect(html).toContain('var(--agent-claude)');
    expect(html).toContain('var(--agent-codex)');
    expect(html).not.toContain('var(--agent-kimi)');
    expect(html).not.toContain('随当前登录而定');
    expect(html).toContain('~/.pi/agent');
    expect(html).toContain('打开该 Agent 的配置目录');
    expect(html).toContain('打开安装目录');
    expect(html).toContain('npm 包');
    expect(html).toContain('npm @earendil-works/pi-coding-agent');
  });

  it('colors ZCode messages, responses, and completions with Claude / Codex tokens', () => {
    const html = renderPanel(installed('zcode', 'native'));
    expect(html).toContain('/v1/messages');
    expect(html).toContain('/v1/responses');
    expect(html).toContain('/v1/chat/completions');
    expect(html).not.toContain('Claude 对话');
    expect(html).not.toContain('Codex / Grok 对话');
    expect(html).not.toContain('Kimi 等补全');
    expect(html).toContain('var(--agent-claude)');
    expect(html).toContain('var(--agent-codex)');
  });

  it('keeps Cursor Agent on the login-dependent line', () => {
    const html = renderPanel(installed('cursor', 'native'));
    expect(html).toContain('端点类型');
    expect(html).toContain('随当前登录而定');
    expect(html).not.toContain('/v1/messages');
    expect(html).not.toContain('/v1/chat/completions');
  });

  it('shows npm 包 as 渠道 and keeps the package id on 安装位置', () => {
    const html = renderPanel(installed('codex', 'npm'));
    expect(html).toContain('>npm 包</button>');
    expect(html).toContain('>npm @openai/codex</button>');
    expect(html).not.toMatch(/>npm</);
    expect(html).not.toContain('渠道</dt><dd class="min-w-0 break-all text-secondary">npm @');
  });

  it('lets a click on the npm package name copy that channel\'s install command', () => {
    const html = renderPanel(installed('dsh', 'npm'));
    expect(html).toContain('>npm @deepseek-ai/dsh</button>');
    expect(html).toContain('复制命令');
  });

  it('puts an upgrade button on the installed channel', () => {
    const html = renderPanel(installed('codex', 'npm'));
    expect(html).toContain('强制升级');
    expect(html).toContain('打开安装目录');
  });

  it('grays the upgrade on a desktop copy and hints to update there', () => {
    const html = renderPanel(installed('codex', 'desktop'));
    expect(html).toContain('请到桌面应用更新');
    expect(html).not.toContain('强制升级');
  });

  it('lists missing Codex npm with the install command and an Install button, without a path', () => {
    const html = renderPanel({
      agentId: 'codex',
      installed: false,
      authStatus: 'none',
      authLabel: '未配置',
      running: false,
    });
    expect(html).toContain('npm @openai/codex');
    expect(html).toContain('npm i -g @openai/codex');
    expect(html).toContain('>npm @openai/codex</button>');
    expect(html).toContain('复制命令');
    expect(html).toContain('安装');
    expect(html).not.toContain('/home/box');
    expect(html).not.toContain('AppData');
    expect(html).not.toContain('打开安装目录');
  });

  it('keeps the installed native path and still offers missing npm install', () => {
    const html = renderPanel(installed('codex', 'native'));
    expect(html).toContain('/home/box/.local/bin/codex');
    expect(html).toContain('打开安装目录');
    expect(html).toContain('仅卸载程序');
    expect(html).toContain('npm i -g @openai/codex');
    expect(html).toContain('复制命令');
    expect(html).toContain('安装');
  });

  it('opens an empty detail for an uninstalled agent', () => {
    const html = renderPanel({
      agentId: 'workbuddy',
      installed: false,
      authStatus: 'none',
      authLabel: '未配置',
      running: false,
    });
    expect(html).toContain('WorkBuddy');
    expect(html).toContain('未安装');
    expect(html).toContain('端点类型');
    expect(html).toContain('/v1/chat/completions');
    expect(html).not.toContain('Kimi 等补全');
    expect(html).toContain('var(--agent-codex)');
    expect(html).not.toContain('随当前登录而定');
    expect(html).toContain('~/.workbuddy');
    expect(html).toContain('卸载并删除配置');
    expect(html).not.toContain('仅卸载程序');
    expect(html).not.toMatch(/>native</);
  });

  it('labels leftover copies as leftover, not as another version', () => {
    const html = renderPanel({
      ...installed('codex', 'npm'),
      extraCopies: [
        {
          path: '/home/box/.agenthub/npm/bin/codex',
          kind: 'leftover-agenthub',
          version: '0.50.0',
        },
      ],
    });
    expect(html).toContain('遗留数据目录 npm');
    expect(html).toContain('/v1/responses');
    expect(html).toContain('勿从此路径启动');
    expect(html).toContain('text-warning');
    expect(html).toContain('border-warning/45');
    expect(html).not.toMatch(/>native</);
    expect(html).not.toMatch(/>npm</);
  });
});
