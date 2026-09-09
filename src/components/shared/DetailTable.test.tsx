import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { DetailRow } from '@/components/shared/DetailRow';
import { DetailTable, DetailTableCell, DetailTableRow } from '@/components/shared/DetailTable';

describe('DetailTable', () => {
  it('renders a headerless table so section titles are not repeated', () => {
    const markup = renderToStaticMarkup(
      createElement(
        DetailTable,
        null,
        createElement(
          DetailTableRow,
          { label: '7 天已用', children: createElement(DetailTableCell, null, '40%') },
        ),
      ),
    );
    expect(markup).toContain('data-detail-table');
    expect(markup).toContain('<table');
    expect(markup).toContain('7 天已用');
    expect(markup).toContain('40%');
    expect(markup).not.toContain('<thead');
    expect(markup).not.toContain('项目');
    expect(markup).not.toContain('内容');
  });

  it('lets DetailRow become a table row inside the table', () => {
    const markup = renderToStaticMarkup(
      createElement(
        DetailTable,
        null,
        createElement(DetailRow, { label: '地址', value: 'https://relay.example.com/v1', mono: true }),
      ),
    );
    expect(markup).toContain('<th');
    expect(markup).toContain('地址');
    expect(markup).toContain('https://relay.example.com/v1');
    expect(markup).toContain('font-mono');
    expect(markup).not.toContain('grid-cols-[');
  });
});
