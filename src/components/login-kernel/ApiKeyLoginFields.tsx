import type { ClipboardEvent } from 'react';
import { SecretInput } from '@/components/shared/SecretInput';
import { Input } from '@/components/ui/input';
import type { ApiKeyFieldValue } from './types';

/**
 * Controlled secret and endpoint fields.
 * Empty secret is left empty so the caller can keep the stored key.
 * This component does not save, submit, trim, or copy the secret into labels.
 */
export function ApiKeyLoginFields({
  mode = 'single',
  value,
  onChange,
  secretLabel,
  endpointLabel,
  secretPlaceholder,
  endpointPlaceholder,
  secretHint,
  endpointHint,
  disabled,
  showSecret = true,
  showEndpoint = true,
  endpointReadOnly = false,
  onEndpointPaste,
  secretRows = 3,
  fieldOrder = 'secret-first',
}: {
  mode?: 'single' | 'multiline';
  value: ApiKeyFieldValue;
  onChange: (next: ApiKeyFieldValue) => void;
  secretLabel?: string;
  endpointLabel?: string;
  secretPlaceholder?: string;
  endpointPlaceholder?: string;
  secretHint?: string;
  endpointHint?: string;
  disabled?: boolean;
  showSecret?: boolean;
  showEndpoint?: boolean;
  endpointReadOnly?: boolean;
  onEndpointPaste?: (event: ClipboardEvent<HTMLInputElement>) => void;
  secretRows?: number;
  fieldOrder?: 'secret-first' | 'endpoint-first';
}) {
  const update = (patch: Partial<ApiKeyFieldValue>) => {
    onChange({
      secret: value.secret,
      endpoint: value.endpoint,
      ...patch,
    });
  };

  const secretField = showSecret ? (
        <label className="flex flex-col gap-1.5">
          {secretLabel ? <span className="text-xs text-muted">{secretLabel}</span> : null}
          {mode === 'multiline' ? (
            <textarea
              value={value.secret}
              onChange={(event) => update({ secret: event.target.value })}
              placeholder={secretPlaceholder}
              rows={secretRows}
              autoComplete="off"
              spellCheck={false}
              disabled={disabled}
              className="min-h-[4.5rem] w-full resize-y rounded-btn border border-border-strong bg-panel px-2.5 py-2 font-mono text-body text-primary placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent/60 disabled:opacity-50"
            />
          ) : (
            <SecretInput
              value={value.secret}
              onChange={(secret) => update({ secret })}
              placeholder={secretPlaceholder}
              disabled={disabled}
            />
          )}
          {secretHint ? <p className="text-meta text-muted">{secretHint}</p> : null}
        </label>
      ) : null;
  const endpointField = showEndpoint ? (
        <label className="flex flex-col gap-1.5">
          {endpointLabel ? <span className="text-xs text-muted">{endpointLabel}</span> : null}
          <Input
            value={value.endpoint}
            onChange={(event) => {
              if (endpointReadOnly) return;
              update({ endpoint: event.target.value });
            }}
            onPaste={onEndpointPaste}
            placeholder={endpointPlaceholder}
            autoComplete="off"
            spellCheck={false}
            disabled={disabled}
            readOnly={endpointReadOnly}
            className={endpointReadOnly ? 'cursor-default bg-canvas text-secondary' : undefined}
          />
          {endpointHint ? <p className="text-meta text-muted">{endpointHint}</p> : null}
        </label>
      ) : null;

  return (
    <div className="flex flex-col gap-3">
      {fieldOrder === 'endpoint-first' ? endpointField : secretField}
      {fieldOrder === 'endpoint-first' ? secretField : endpointField}
    </div>
  );
}
