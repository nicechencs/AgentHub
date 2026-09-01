import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { Provider } from '@/lib/types';
import { ApiAccessDialog } from './ApiAccessDialog';

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

const PROVIDER: Provider = {
  id: 'p-mytokens',
  agentId: 'codex',
  name: 'mytokens.cc /v1/responses',
  preset: 'custom',
  configText: 'base_url = "https://mytokens.cc/v1"\napi_key = "***"\n',
  configFormat: 'toml',
  isCurrent: false,
};

function render(edit?: { provider: Provider; endpointKinds: readonly ['responses_codex'] }) {
  return renderToStaticMarkup(
    createElement(ApiAccessDialog, {
      open: true,
      agents: ['claude', 'codex', 'grok'],
      edit: edit ?? null,
      onOpenChange() {},
    }),
  );
}

describe('ApiAccessDialog', () => {
  it('lists API types when adding a login', () => {
    const markup = render();
    expect(markup).toContain('接入 API');
    expect(markup).toContain('接口类型');
    expect(markup).toContain('/v1/messages');
    expect(markup).toContain('/v1/responses');
    expect(markup).toContain('/v1/chat/completions');
    expect(markup).toContain('type="checkbox"');
    expect(markup).not.toContain('接入时已定好，编辑时不能改');
  });

  it('explains locked API types instead of listing them when editing', () => {
    const markup = render({
      provider: PROVIDER,
      endpointKinds: ['responses_codex'],
    });
    expect(markup).toContain('编辑密钥');
    expect(markup).toContain('接口类型');
    expect(markup).toContain('接入时已定好，编辑时不能改。要换类型请再接入一次。');
    expect(markup).not.toContain('type="checkbox"');
    expect(markup).not.toContain('填完服务地址和 API Key 后，会自动侦测可用接口类型');
  });
});
