import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { DetailsToggle } from './DetailsToggle';

describe('DetailsToggle', () => {
  it('renders 详情 with a chevron and collapsed aria state', () => {
    const html = renderToStaticMarkup(
      createElement(DetailsToggle, { open: false, controlsId: 'row-details', children: '详情' }),
    );
    expect(html).toContain('详情');
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain('aria-controls="row-details"');
    expect(html).toContain('<svg');
    expect(html).not.toContain('rotate-180');
    expect(html).toContain('data-btn="ghost"');
    expect(html).toContain('data-btn-size="sm"');
  });

  it('rotates the chevron when expanded', () => {
    const html = renderToStaticMarkup(
      createElement(DetailsToggle, { open: true, controlsId: 'row-details', children: '详情' }),
    );
    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain('rotate-180');
  });
});
