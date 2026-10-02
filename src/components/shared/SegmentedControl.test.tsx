import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@/components/ui/tooltip', () => ({
  Hint: ({ children }: { children: unknown }) => children,
}));

import { SegmentedControl } from './SegmentedControl';

function render(value: string, options: Array<{ value: string; label: string; disabled?: boolean }>) {
  return renderToStaticMarkup(createElement(SegmentedControl, {
    value,
    options,
    onChange: () => {},
    'aria-label': '调度',
  }));
}

describe('SegmentedControl', () => {
  it('makes the first enabled option tabbable when there is no active option', () => {
    const markup = render('', [
      { value: 'disabled', label: 'Disabled', disabled: true },
      { value: 'first', label: 'First' },
      { value: 'second', label: 'Second' },
    ]);

    expect(markup).toContain('aria-selected="false" tabindex="-1" disabled=""');
    expect(markup).toContain('aria-selected="false" tabindex="0"');
    expect(markup.match(/aria-selected="true"/g)).toBeNull();
  });

  it('keeps the selected option tabbable', () => {
    const markup = render('second', [
      { value: 'first', label: 'First' },
      { value: 'second', label: 'Second' },
    ]);

    expect(markup).toContain('aria-selected="false" tabindex="-1"');
    expect(markup).toContain('aria-selected="true" tabindex="0"');
  });
});
