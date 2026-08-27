import { createElement } from 'react';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import type { AgentConfigSchemaDto } from '@/lib/backend/contracts/config-types';
import { SECRET_REDACTED } from '@/lib/backend/contracts/config-types';
import { GenericConfigForm } from './GenericConfigForm';

const dir = path.dirname(fileURLToPath(import.meta.url));

function formSource(): string {
  return readFileSync(path.join(dir, 'GenericConfigForm.tsx'), 'utf8');
}

const secretSchema: AgentConfigSchemaDto = {
  agentKey: 'claude',
  schemaVersion: 1,
  nativeFormat: 'json',
  relativePath: 'settings.json',
  fields: [
    { key: 'apiKey', label: 'API Key', valueType: { kind: 'secret' }, secret: true },
  ],
};

function renderSecretForm(opts: {
  disabled?: boolean;
  readOnlyKeys?: string[];
  value?: string;
}): string {
  return renderToStaticMarkup(
    createElement(
      TooltipProvider,
      null,
      createElement(GenericConfigForm, {
        schema: secretSchema,
        values: { apiKey: opts.value ?? '' },
        onChange: () => {},
        disabled: opts.disabled,
        readOnlyKeys: opts.readOnlyKeys,
      }),
    ),
  );
}

describe('GenericConfigForm renderer', () => {
  it('does not hard-code Connections picker keys or secret kind checks', () => {
    const src = formSource();
    expect(src).not.toContain('connections.providerDialog.remoteModelsPick');
    expect(src).not.toContain('connections.providerDialog.remoteModelsCustom');
    expect(src).not.toContain('field.secret');
    expect(src).not.toContain("valueType.kind === 'secret'");
    expect(src).toContain('fieldControlKind');
    expect(src).toContain('pickLabel');
    expect(src).toContain('customLabel');
  });

  it('passes disabled and readOnly to SecretInput when kind is secret', () => {
    const src = formSource();
    const secretBlock = src.slice(src.indexOf("kind === 'secret'"), src.indexOf("kind === 'string'"));
    expect(secretBlock).toContain('<SecretInput');
    expect(secretBlock).toContain('disabled={fieldDisabled}');
    expect(secretBlock).toContain('readOnly={fieldDisabled}');

    const locked = renderSecretForm({ disabled: true, value: SECRET_REDACTED });
    const inputs = locked.match(/<input\b[^>]*>/g) ?? [];
    expect(
      inputs.some((tag) => /\bdisabled\b/i.test(tag) && /\breadonly\b/i.test(tag)),
    ).toBe(true);
    expect(locked).toMatch(/<button\b[^>]*\bdisabled\b/i);

    const readOnly = renderSecretForm({ readOnlyKeys: ['apiKey'] });
    const readOnlyInputs = readOnly.match(/<input\b[^>]*>/g) ?? [];
    expect(
      readOnlyInputs.some((tag) => /\bdisabled\b/i.test(tag) && /\breadonly\b/i.test(tag)),
    ).toBe(true);
  });
});
