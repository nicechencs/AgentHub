import { createElement } from 'react';
import type { SVGProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';

type MockIconProps = SVGProps<SVGSVGElement> & {
  absoluteStrokeWidth?: boolean;
  size?: number;
};

vi.mock('lucide-react', () => ({
  Download: ({ size, strokeWidth, absoluteStrokeWidth, ...props }: MockIconProps) =>
    createElement('svg', {
      ...props,
      'data-icon': 'download',
      'data-size': size,
      'data-stroke-width': strokeWidth,
      'data-absolute-stroke-width': absoluteStrokeWidth,
    }),
  Wrench: ({ size, strokeWidth, absoluteStrokeWidth, ...props }: MockIconProps) =>
    createElement('svg', {
      ...props,
      'data-icon': 'wrench',
      'data-size': size,
      'data-stroke-width': strokeWidth,
      'data-absolute-stroke-width': absoluteStrokeWidth,
    }),
}));

import { AgentInstallButton } from './AgentInstallButton';

function renderButton(status?: 'failed' | 'guided') {
  return renderToStaticMarkup(
    createElement(
      TooltipProvider,
      null,
      createElement(AgentInstallButton, { status, iconOnly: true, onClick() {} }),
    ),
  );
}

describe('AgentInstallButton', () => {
  function expectIconMetrics(html: string) {
    expect(html).toContain('data-size="16"');
    expect(html).toContain('data-stroke-width="1.6"');
    expect(html).toContain('data-absolute-stroke-width="true"');
  }

  it('uses a download icon and the install label for a normal installation', () => {
    const html = renderButton();
    expect(html).toContain('data-icon="download"');
    expect(html).toContain('aria-label="安装"');
    expect(html).toContain('data-btn="secondary"');
    expect(html).not.toContain('data-btn="default"');
    expectIconMetrics(html);
  });

  it.each([
    ['failed', '重试'],
    ['guided', '重新检测'],
  ] as const)('uses a repair icon and retains the %s action label', (status, label) => {
    const html = renderButton(status);
    expect(html).toContain('data-icon="wrench"');
    expect(html).toContain(`aria-label="${label}"`);
    expect(html).toContain('data-btn="secondary"');
    expect(html).not.toContain('data-btn="default"');
    expectIconMetrics(html);
  });
});
