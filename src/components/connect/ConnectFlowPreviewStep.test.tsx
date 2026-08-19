import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { AdapterApplyPlan, AdapterRouteAnalysis } from '@/lib/api/adapter';
import type { ConnectFlowEntry } from '@/lib/connect-flow/types';
import { ConnectFlowPreviewStep } from './ConnectFlowPreviewStep';
import { createConnectFlowState, reduceConnectFlow, type ConnectFlowState } from './connect-flow-state';

function analysis(overrides: Partial<AdapterRouteAnalysis> = {}): AdapterRouteAnalysis {
  return {
    route: 'local_bridge',
    support: 'experimental',
    reason: 'Grok 登录会经本机路由接到 Claude Code。',
    actions: [],
    limitations: [],
    evidence: [],
    ...overrides,
  };
}

function plan(overrides: Partial<AdapterApplyPlan> = {}): AdapterApplyPlan {
  return {
    analysis: analysis(),
    targetAgentId: 'claude',
    canApply: true,
    serviceImpact: 'requires_local_bridge',
    changes: [
      { target: 'claude', field: 'ANTHROPIC_BASE_URL', value: 'http://127.0.0.1:<本机端口>', secret: false },
    ],
    ...overrides,
  };
}

function previewState(): ConnectFlowState {
  const entry: ConnectFlowEntry = { mode: 'for-source', source: { kind: 'account', id: 'acc-grok' } };
  let state = createConnectFlowState(entry);
  state = reduceConnectFlow(state, {
    type: 'select_target',
    agentId: 'claude',
    sourceAgentId: 'grok',
  });
  return reduceConnectFlow(state, {
    type: 'enter_preview',
    eligibility: {
      kind: 'ready',
      plan: plan(),
      canApply: true,
      routeSummary: '③ 本机协议桥',
    },
  });
}

const FOOTER = '去 Connections 导入';
const BANNED_PREVIEW_COPY = [
  'ANTHROPIC_',
  'Messages',
  '将写入的配置',
  '可应用',
  '③ 本机协议桥',
  '127.0.0.1',
];

describe('ConnectFlowPreviewStep import footer', () => {
  it('hides the import hint on a live-ticket path', () => {
    const html = renderToStaticMarkup(createElement(ConnectFlowPreviewStep, {
      state: previewState(),
      option: null,
      previewInvalid: false,
      showImportHint: false,
      onGoImport: () => undefined,
    }));
    expect(html).not.toContain(FOOTER);
  });

  it('shows the import hint when the source is not logged in', () => {
    const html = renderToStaticMarkup(createElement(ConnectFlowPreviewStep, {
      state: previewState(),
      option: null,
      previewInvalid: false,
      showImportHint: true,
      onGoImport: () => undefined,
    }));
    expect(html).toContain(FOOTER);
  });
});

describe('ConnectFlowPreviewStep Grok→Claude markup', () => {
  it('shows 本机路由 copy and hides plan dumps', () => {
    const html = renderToStaticMarkup(createElement(ConnectFlowPreviewStep, {
      state: previewState(),
      option: null,
      previewInvalid: false,
      showImportHint: false,
      onGoImport: () => undefined,
    }));
    expect(html).toContain('本机路由');
    expect(html).toContain('实验');
    expect(html).toContain('请保持 AgentHub');
    expect(html).toContain('不会进 Claude');
    for (const banned of BANNED_PREVIEW_COPY) {
      expect(html).not.toContain(banned);
    }
  });
});
