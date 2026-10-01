import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { ApiKeyLoginFields } from './ApiKeyLoginFields';
import type { ApiKeyFieldValue } from './types';

const secret = 'sk-test-SECRETVALUE';

function markup(props: {
  mode?: 'single' | 'multiline';
  value: ApiKeyFieldValue;
  onChange?: (next: ApiKeyFieldValue) => void;
}) {
  return renderToStaticMarkup(createElement(
    TooltipProvider,
    null,
    createElement(ApiKeyLoginFields, {
      mode: props.mode ?? 'single',
      value: props.value,
      onChange: props.onChange ?? (() => {}),
      secretLabel: 'API Key',
      endpointLabel: '端点',
      secretPlaceholder: '留空则保留原密钥',
      endpointPlaceholder: 'https://api.example.com',
      secretHint: '留空则不覆盖',
    }),
  ));
}

describe('ApiKeyLoginFields', () => {
  it('renders single and multiline values as controlled fields', () => {
    const single = markup({ value: { secret, endpoint: 'https://api.example.com/v1' } });
    expect(single).toContain('type="password"');
    expect(single).toContain(`value="${secret}"`);
    expect(single).toContain('value="https://api.example.com/v1"');
    expect(single).not.toContain('<form');
    expect(single).not.toContain('type="submit"');

    const many = markup({
      mode: 'multiline',
      value: { secret: `${secret}\n${secret}-2`, endpoint: 'https://api.example.com' },
    });
    expect(many).toContain('<textarea');
    expect(many).toContain(`${secret}\n${secret}-2`);
    expect(many).toContain('value="https://api.example.com"');
  });

  it('leaves an empty edit value empty and does not echo the secret outside the field', () => {
    const empty = markup({ value: { secret: '', endpoint: '' } });
    expect(empty).toContain('value=""');
    expect(empty).not.toContain(secret);
    expect(empty).toContain('留空则保留原密钥');
    expect(empty).toContain('留空则不覆盖');

    const shown = markup({ value: { secret, endpoint: 'https://api.example.com' } });
    expect(shown).not.toContain(`>${secret}<`);
    expect(shown).toContain('API Key');
    expect(shown).toContain('端点');
  });

  it('does not trim or submit from the change callback', () => {
    const onChange = vi.fn();
    markup({ value: { secret: `  ${secret}  `, endpoint: ' https://api.example.com ' }, onChange });
    expect(onChange).not.toHaveBeenCalled();
    const html = markup({ value: { secret: `  ${secret}  `, endpoint: ' https://api.example.com ' } });
    expect(html).toContain(`value="  ${secret}  "`);
    expect(html).toContain('value=" https://api.example.com "');
  });
});
