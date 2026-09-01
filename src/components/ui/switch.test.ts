import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { Switch } from './switch';

function switchMarkup() {
  return renderToStaticMarkup(createElement(Switch));
}

describe('Switch', () => {
  it('keeps a 4px inset around the thumb inside the track', () => {
    const html = switchMarkup();
    expect(html).toContain('p-1');
    expect(html).toContain('h-4 w-4');
    expect(html).toContain('data-[state=checked]:translate-x-4');
    expect(html).not.toContain('before:');
  });

  it('uses a distinct off-track fill instead of the surrounding subtle surface', () => {
    const html = switchMarkup();
    expect(html).toContain('bg-active');
    expect(html).toContain('data-[state=checked]:bg-accent');
    expect(html).not.toContain('bg-subtle');
    expect(html).not.toContain('bg-transparent');
  });
});
