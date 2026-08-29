import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { AGENT_MAP } from '@/config/agents';
import { translate } from '@/lib/i18n';
import { zh } from '@/lib/i18n/locales/zh';
import { en } from '@/lib/i18n/locales/en';
import { extraCopyKindLabel, extraCopyKindLabelKey } from './agent-card-model';
import { AgentDetailPanel } from './AgentDetailPanel';
import {
  agentConversationEndpoints,
  agentConversationSurface,
  catalogChannelLabel,
  displayAgentConfigDir,
  formatAgentConversationEndpoints,
  formatConversationEndpointLabel,
  installChannelDisplayLabel,
  isRawInstallChannelLabel,
} from './agent-detail-model';
import type { AgentStatus } from '@/lib/types';

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

describe('agent conversation surfaces (dest RouteDownstreamSurface::for_agent)', () => {
  it('maps catalog agents onto the conversation path they actually support', () => {
    expect(agentConversationSurface('claude')).toBe('messages');
    expect(agentConversationSurface('codex')).toBe('responses');
    expect(agentConversationSurface('grok')).toBe('responses');
    expect(agentConversationSurface('kimi')).toBe('chat_completions');
    expect(agentConversationSurface('dsh')).toBe('chat_completions');
    expect(agentConversationEndpoints('claude')).toEqual([
      {
        id: 'messages',
        path: '/v1/messages',
        copyKey: 'routes.endpoint.messages',
      },
    ]);
    expect(agentConversationEndpoints('codex')[0]?.path).toBe('/v1/responses');
    expect(agentConversationEndpoints('grok')[0]?.path).toBe('/v1/responses');
    expect(agentConversationEndpoints('kimi')[0]?.path).toBe('/v1/chat/completions');
    expect(agentConversationEndpoints('dsh')[0]?.path).toBe('/v1/chat/completions');
  });

  it('does not invent a default surface when dest has none', () => {
    expect(agentConversationSurface('pi')).toBeNull();
    expect(agentConversationSurface('workbuddy')).toBeNull();
    expect(agentConversationSurface('zcode')).toBeNull();
    expect(agentConversationSurface('cursor')).toBeNull();
    expect(agentConversationEndpoints('pi')).toEqual([]);
    expect(formatAgentConversationEndpoints('pi', tZh)).toBe('随当前登录而定');
    expect(formatAgentConversationEndpoints('pi', tEn)).toBe('Depends on the current login');
  });

  it('reuses dest route endpoint copy for zh and en', () => {
    const claude = agentConversationEndpoints('claude')[0]!;
    expect(formatConversationEndpointLabel(claude, tZh)).toBe('/v1/messages · Claude 对话');
    expect(formatConversationEndpointLabel(claude, tEn)).toBe('/v1/messages · Claude chat');
    expect(formatAgentConversationEndpoints('grok', tZh)).toBe('/v1/responses · Codex / Grok 对话');
    expect(formatAgentConversationEndpoints('kimi', tZh)).toBe('/v1/chat/completions · Kimi 等补全');
    expect(formatAgentConversationEndpoints('dsh', tEn)).toBe(
      '/v1/chat/completions · Kimi and other completions',
    );
    expect(zh.routes.endpoint.messages).toBe('Claude 对话');
    expect(zh.routes.pool.surface.messages).toBe('对话接口');
    expect(en.routes.endpoint.responses).toBe('Codex / Grok chat');
  });
});

describe('install channel labels', () => {
  it('never treats raw native as product copy', () => {
    expect(isRawInstallChannelLabel('native', 'native')).toBe(true);
    expect(isRawInstallChannelLabel('native', 'native 官方脚本')).toBe(true);
    expect(isRawInstallChannelLabel('native', '官网 Setup（打开安装页）')).toBe(false);
    expect(isRawInstallChannelLabel('npm', 'npm')).toBe(true);
    expect(isRawInstallChannelLabel('npm', 'npm @anthropic-ai/claude-code')).toBe(false);
  });

  it('prefers catalog labels and falls back to official-script / npm words', () => {
    expect(catalogChannelLabel('claude', 'native')).toBeUndefined();
    expect(catalogChannelLabel('workbuddy', 'native')).toBe('官网 Setup（打开安装页）');
    expect(catalogChannelLabel('claude', 'npm')).toBe('npm @anthropic-ai/claude-code');
    expect(installChannelDisplayLabel('claude', 'native', tZh)).toBe('官方脚本');
    expect(installChannelDisplayLabel('claude', 'native', tEn)).toBe('Official script');
    expect(installChannelDisplayLabel('claude', 'npm', tZh)).toBe('npm @anthropic-ai/claude-code');
    expect(installChannelDisplayLabel('workbuddy', 'native', tZh)).toBe('官网 Setup（打开安装页）');
    expect(installChannelDisplayLabel('claude', 'ide', tZh)).toBe('IDE 插件');
    expect(installChannelDisplayLabel('claude', 'desktop', tZh)).toBe('桌面应用');
    expect(installChannelDisplayLabel('claude', undefined, tZh)).toBeUndefined();
    expect(AGENT_MAP.claude?.installChannels.some((row) => row.id === 'native')).toBe(true);
    expect(extraCopyKindLabelKey('native')).toBe('agents.card.channelOfficial');
    expect(extraCopyKindLabel('native', tZh)).toBe('官方脚本');
    expect(extraCopyKindLabel('npm', tZh)).toBe('npm');
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
    const html = renderToStaticMarkup(
      createElement(AgentDetailPanel, {
        agent: installed('claude'),
        width: 360,
        onClose: () => undefined,
        onChanged: () => undefined,
      }),
    );
    expect(html).toContain('端点类型');
    expect(html).toContain('/v1/messages');
    expect(html).toContain('Claude 对话');
    expect(html).toContain('官方脚本');
    expect(html).not.toMatch(/>native</);
    expect(html).toContain('~/.claude');
    expect(html).not.toMatch(/配置目录<\/span>/);
  });

  it('keeps an honest empty endpoint line for Pi and does not invent a path style', () => {
    const html = renderToStaticMarkup(
      createElement(AgentDetailPanel, {
        agent: installed('pi', 'npm'),
        width: 360,
        onClose: () => undefined,
        onChanged: () => undefined,
      }),
    );
    expect(html).toContain('端点类型');
    expect(html).toContain('随当前登录而定');
    expect(html).not.toContain('/v1/chat/completions');
    expect(html).not.toContain('/v1/messages');
    expect(html).toContain('~/.pi/agent');
  });
});
