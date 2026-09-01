import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import { createTranslator } from '@/lib/i18n';
import { WriteClientConfigDialog } from './WriteClientConfigDialog';
import type { CreateRouteTarget } from './create-route-flow';
import type { RouteGraphRow } from './route-graph-model';

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

const t = createTranslator('zh');
const PORT = 43121;

const PROFILE: AdapterProfile = {
  id: 'bridge-1',
  name: 'OpenRouter 备选',
  sourceKind: 'provider',
  sourceId: 'openrouter-1',
  targetAgentId: 'codex',
  route: 'local_bridge',
  mode: 'api',
  status: 'active',
  ruleId: 'openai-api-to-codex-v1',
  ruleVersion: '1',
  generatedProviderId: 'codex-bridge-1',
  localPort: PORT,
  autoStart: true,
  createdAt: '2026-08-24T00:00:00Z',
  updatedAt: '2026-08-24T00:00:00Z',
};

function row(agent: CreateRouteTarget, overrides: Partial<RouteGraphRow> = {}): RouteGraphRow {
  const messages = agent === 'claude';
  return {
    agent,
    localPath: messages ? '/v1/messages' : '/v1/responses',
    localEndpointId: messages ? 'messages' : 'responses',
    localUrl: `http://127.0.0.1:${PORT}${messages ? '/v1/messages' : '/v1/responses'}`,
    upstreamBaseUrl: 'https://openrouter.ai/api/v1',
    upstreamPath: '/chat/completions',
    upstreamUrl: 'https://openrouter.ai/api/v1/chat/completions',
    upstreamChannel: 'openai_chat',
    hop: 'convert',
    link: 'dashed',
    enabled: true,
    applied: false,
    ...overrides,
  };
}

function render(props: {
  profile?: AdapterProfile | null;
  rows?: readonly RouteGraphRow[];
  port?: number | null;
  sourceMissing?: boolean;
  hiddenTargetIds?: ReadonlySet<string>;
  listedModels?: readonly string[];
  contextWindowTokens?: number | null;
}): string {
  return renderToStaticMarkup(
    createElement(TooltipProvider, null, createElement(WriteClientConfigDialog, {
      open: true,
      onOpenChange: vi.fn(),
      profile: props.profile === undefined ? PROFILE : props.profile,
      rows: props.rows ?? [],
      host: '127.0.0.1',
      port: props.port === undefined ? PORT : props.port,
      sourceMissing: props.sourceMissing ?? false,
      hiddenTargetIds: props.hiddenTargetIds,
      listedModels: props.listedModels,
      contextWindowTokens: props.contextWindowTokens,
      onWritten: vi.fn(),
    })),
  );
}

/** The header copy names both endpoints, so per-row assertions read the body only. */
function body(markup: string): string {
  return markup.split(t('routes.write.description')).join('');
}

function checkboxTags(markup: string): string[] {
  return markup.match(/<input[^>]*>/g) ?? [];
}

function countOf(markup: string, needle: string): number {
  return markup.split(needle).length - 1;
}

describe('WriteClientConfigDialog', () => {
  it('shows Codex and Grok config files without wire_api or api_backend', () => {
    const markup = render({ rows: [row('claude'), row('codex'), row('grok')] });

    expect(markup).toContain(t('routes.write.title'));
    expect(markup).toContain(t('routes.write.description'));
    expect(markup).toContain(t('routes.pool.surface.messages'));
    expect(markup).toContain(t('routes.pool.surface.responsesCodex'));
    expect(markup).toContain(t('routes.pool.surface.responsesGrok'));
    expect(markup).toContain(t('routes.endpoint.modelsLine'));
    expect(markup).toContain('~/.codex/config.toml');
    expect(markup).toContain('~/.grok/config.toml');
    expect(markup).not.toContain('wire_api');
    expect(markup).not.toContain('api_backend');

    const claudeOnly = body(render({ rows: [row('claude')] }));
    expect(claudeOnly).toContain('http://127.0.0.1:43121/v1/messages');
    expect(claudeOnly).not.toContain('/v1/responses');
    expect(claudeOnly).not.toContain('wire_api');
    expect(claudeOnly).not.toContain('api_backend');

    const codexOnly = body(render({ rows: [row('codex')] }));
    expect(codexOnly).toContain('http://127.0.0.1:43121/v1/responses');
    expect(codexOnly).not.toContain('/v1/messages');
    expect(codexOnly).toContain('~/.codex/config.toml');
    expect(codexOnly).not.toContain('api_backend');
    expect(codexOnly).not.toContain('~/.grok/config.toml');

    const grokOnly = body(render({ rows: [row('grok')] }));
    expect(grokOnly).toContain('http://127.0.0.1:43121/v1/responses');
    expect(grokOnly).not.toContain('/v1/messages');
    expect(grokOnly).toContain('~/.grok/config.toml');
    expect(grokOnly).not.toContain('wire_api');
    expect(grokOnly).not.toContain('~/.codex/config.toml');
  });

  it('previews Claude model and 1M window when the route declares them', () => {
    const markup = render({
      rows: [row('claude')],
      listedModels: ['stealth/ox-alpha'],
      contextWindowTokens: 1_048_576,
    });
    expect(markup).toContain(t('routes.write.fieldModel'));
    expect(markup).toContain('stealth/ox-alpha');
    expect(markup).toContain(t('routes.write.fieldContextWindow'));
    expect(markup).toContain('1M');
  });

  it('names Claude settings.json with a local address and token, not env keys', () => {
    const markup = render({ rows: [row('claude'), row('codex'), row('grok')] });

    expect(markup).toContain('~/.claude/settings.json');
    expect(markup).toContain(t('routes.write.fieldLocalAddress'));
    expect(markup).toContain(t('routes.write.fieldLocalToken'));
    expect(markup).toContain(t('routes.write.localToken'));
    expect(markup).not.toContain('env.ANTHROPIC_BASE_URL');
    expect(markup).not.toContain('env.ANTHROPIC_AUTH_TOKEN');
    expect(markup).not.toContain('sk-');
    expect(markup).not.toContain('ahb_');
  });

  it('disables a row whose route does not open that endpoint', () => {
    const markup = render({ rows: [row('codex', { enabled: false })] });

    expect(markup).toContain(t('routes.write.status.noUpstream', { name: 'Codex' }));
    expect(markup).toContain('opacity-70');
    const boxes = checkboxTags(markup);
    expect(boxes).toHaveLength(1);
    expect(boxes[0]).toContain('disabled');
  });

  it('locks every row when the source login is gone', () => {
    const rows = [row('claude'), row('codex'), row('grok')];
    const markup = render({ rows, sourceMissing: true });

    expect(countOf(markup, t('routes.write.status.sourceMissing'))).toBe(rows.length);
    const boxes = checkboxTags(markup);
    expect(boxes).toHaveLength(rows.length);
    for (const box of boxes) expect(box).toContain('disabled');
    expect(markup).toContain(t('routes.write.submit', { count: 0 }));
  });

  it('hints that the port is still pending', () => {
    const pending = render({ rows: [row('codex')], port: null });
    expect(pending).toContain(t('routes.write.portPending'));

    const assigned = render({ rows: [row('codex')] });
    expect(assigned).not.toContain(t('routes.write.portPending'));
    expect(assigned).toContain(`127.0.0.1:${PORT}`);
  });

  it('renders nothing without a profile', () => {
    expect(render({ profile: null, rows: [row('codex')] })).toBe('');
  });
});
