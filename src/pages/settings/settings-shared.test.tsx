import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { SettingsRow } from './settings-shared';

describe('SettingsRow', () => {
  it('points the first control at the visible label and description', () => {
    const html = renderToStaticMarkup(
      createElement(
        SettingsRow,
        { label: '开机自启', description: '登录后自动打开' },
        createElement('button', { type: 'button', role: 'switch' }, 'on'),
      ),
    );
    expect(html).toMatch(/id="[^"]+-label"/);
    expect(html).toContain('开机自启');
    expect(html).toContain('登录后自动打开');
    expect(html).toMatch(/aria-labelledby="[^"]+-label"/);
    expect(html).toMatch(/aria-describedby="[^"]+-desc"/);
  });
});
