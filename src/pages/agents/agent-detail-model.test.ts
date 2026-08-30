import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { AGENT_MAP } from '@/config/agents';
import { translate } from '@/lib/i18n';
import { zh } from '@/lib/i18n/locales/zh';
import { en } from '@/lib/i18n/locales/en';
import { TooltipProvider } from '@/components/ui/tooltip';
import { extraCopyKindLabel, extraCopyKindLabelKey } from './agent-card-model';
import { AgentDetailPanel } from './AgentDetailPanel';
import {
  agentConversationEndpoints,
  agentConversationSurface,
  agentConversationSurfaces,
  catalogChannelLabel,
  displayAgentConfigDir,
  formatAgentConversationEndpoints,
  formatConversationEndpointLabel,
  installChannelKindLabel,
  installLocationSourceLabel,
  isNpmPackageCatalogLabel,
  isRawInstallChannelLabel,
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
    expect(agentConversationSurface('kimi')).toBe('chat_completions');
    expect(agentConversationSurface('dsh')).toBe('messages');
    expect(agentConversationSurface('workbuddy')).toBe('chat_completions');
    expect(agentConversationEndpoints('claude')).toEqual([
      {
        id: 'messages',
        path: '/v1/messages',
        copyKey: 'agents.detail.endpointMessages',
        brandAgentId: 'claude',
      },
    ]);
    expect(agentConversationEndpoints('codex')[0]?.path).toBe('/v1/responses');
    expect(agentConversationEndpoints('grok')[0]).toMatchObject({
      path: '/v1/responses',
      brandAgentId: 'codex',
    });
    expect(agentConversationEndpoints('kimi')[0]?.path).toBe('/v1/chat/completions');
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
      [
        '/v1/messages · Claude 对话',
        '/v1/responses · Codex / Grok 对话',
        '/v1/chat/completions · Kimi 等补全',
      ].join('\n'),
    );
  });

  it('lists ZCode Anthropic + OpenAI and Pi login-dependent surfaces', () => {
    expect(agentConversationSurfaces('zcode')).toEqual(['messages', 'chat_completions']);
    expect(agentConversationEndpoints('zcode').map((row) => [row.id, row.brandAgentId])).toEqual([
      ['messages', 'claude'],
      ['chat_completions', 'codex'],
    ]);
    expect(agentConversationSurfaces('pi')).toEqual([
      'messages',
      'responses',
      'chat_completions',
    ]);
    expect(agentConversationEndpoints('pi').map((row) => row.path)).toEqual([
      '/v1/messages',
      '/v1/responses',
      '/v1/chat/completions',
    ]);
    expect(formatAgentConversationEndpoints('pi', tZh)).toBe(
      [
        '/v1/messages · Claude 对话',
        '/v1/responses · Codex / Grok 对话',
        '/v1/chat/completions · Kimi 等补全',
      ].join('\n'),
    );
  });

  it('does not invent a public HTTP surface for Cursor Agent', () => {
    expect(agentConversationSurface('cursor')).toBeNull();
    expect(agentConversationEndpoints('cursor')).toEqual([]);
    expect(formatAgentConversationEndpoints('cursor', tZh)).toBe('随当前登录而定');
    expect(formatAgentConversationEndpoints('cursor', tEn)).toBe('Depends on the current login');
  });

  it('uses Agents-page endpoint copy for zh and en', () => {
    const claude = agentConversationEndpoints('claude')[0]!;
    expect(formatConversationEndpointLabel(claude, tZh)).toBe('/v1/messages · Claude 对话');
    expect(formatConversationEndpointLabel(claude, tEn)).toBe('/v1/messages · Claude chat');
    expect(formatAgentConversationEndpoints('grok', tZh)).toBe('/v1/responses · Codex / Grok 对话');
    expect(formatAgentConversationEndpoints('kimi', tZh)).toBe('/v1/chat/completions · Kimi 等补全');
    expect(formatAgentConversationEndpoints('workbuddy', tEn)).toBe(
      '/v1/chat/completions · Kimi and other completions',
    );
    expect(zh.agents.detail.endpointMessages).toBe('Claude 对话');
    expect(zh.agents.detail.endpointResponses).toBe('Codex / Grok 对话');
    expect(zh.agents.detail.endpointChatCompletions).toBe('Kimi 等补全');
    expect(en.agents.detail.endpointMessages).toBe('Claude chat');
    expect(en.agents.detail.endpointResponses).toBe('Codex / Grok chat');
    expect(en.agents.detail.endpointChatCompletions).toBe('Kimi and other completions');
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
    expect(html).toContain('Claude 对话');
    expect(html).toContain('var(--agent-claude)');
    expect(html).toContain('官方脚本');
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

  it('colors ZCode messages and completions with Claude / Codex tokens', () => {
    const html = renderPanel(installed('zcode', 'native'));
    expect(html).toContain('/v1/messages');
    expect(html).toContain('/v1/chat/completions');
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
    expect(html).toContain('npm 包');
    expect(html).toContain('npm @openai/codex');
    expect(html).not.toMatch(/>npm</);
    expect(html).not.toContain('渠道</dt><dd class="min-w-0 break-all text-secondary">npm @');
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
    expect(html).toContain('Kimi 等补全');
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
    expect(html).not.toMatch(/>native</);
    expect(html).not.toMatch(/>npm</);
  });
});
