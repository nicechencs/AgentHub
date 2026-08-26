/**
 * Schema-driven config field list (design-system controls).
 * Unknown field types render unavailable; does not parse JSON/TOML.
 */
import * as React from 'react';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { SecretInput } from '@/components/shared/SecretInput';
import { Hint } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import type {
  AgentConfigSchemaDto,
  ConfigValidationIssueDto,
} from '@/lib/backend/contracts/config-types';
import { SECRET_REDACTED } from '@/lib/backend/contracts/config-types';
import { cn } from '@/lib/utils';
import {
  fieldControlKind,
  issuesByField,
  type FormValues,
} from './generic-config-form-map';
import { configFieldHint, configFieldLabel, configFieldOptionLabel } from './config-field-copy';

export interface GenericConfigFormProps {
  schema: AgentConfigSchemaDto;
  values: FormValues;
  onChange: (next: FormValues) => void;
  /** Field-level validation issues from backend. */
  issues?: ConfigValidationIssueDto[];
  disabled?: boolean;
  className?: string;
  /** Optional: hide specific keys (e.g. official mode locks baseUrl/model). */
  readOnlyKeys?: ReadonlySet<string> | string[];
  hiddenKeys?: ReadonlySet<string> | string[];
  /** Optional picker ids for string fields (e.g. fetched model list). Free-text still allowed. */
  suggestions?: Readonly<Record<string, readonly string[]>>;
  /** Optional action rendered immediately after a field, outside its label control. */
  fieldActions?: Readonly<Record<string, React.ReactNode>>;
  /** Status under a string field (shown even when the picker is hidden). */
  fieldStatus?: Readonly<
    Record<string, { label?: string | null; onRetry?: () => void }>
  >;
  /** Visible hint under a field (overrides the beginner field hint when both exist). */
  fieldHints?: Readonly<Record<string, string | undefined>>;
}

const CUSTOM_SUGGESTION = '__agenthub_custom__';

/** String input plus an optional convenience picker. Empty / free-text stay allowed. */
export function SuggestableInput({
  value,
  onChange,
  suggestions,
  disabled,
  readOnly,
  placeholder,
  className,
  statusLabel,
  statusRetry,
}: {
  value: string;
  onChange: (value: string) => void;
  suggestions?: readonly string[];
  disabled?: boolean;
  readOnly?: boolean;
  placeholder?: string;
  className?: string;
  /** Shown even when `suggestions` is empty (loading / fail / empty-ok). */
  statusLabel?: string | null;
  /** When set, `statusLabel` is the primary retry action. */
  statusRetry?: () => void;
}) {
  const { t } = useI18n();
  const opts = (suggestions ?? []).map((id) => id.trim()).filter(Boolean);
  const showPicker = opts.length > 0 && !disabled && !readOnly;
  const selected = value && opts.includes(value) ? value : CUSTOM_SUGGESTION;

  return (
    <div className="flex flex-col gap-1.5">
      {showPicker ? (
        <Select
          value={selected}
          onValueChange={(next) => {
            if (next === CUSTOM_SUGGESTION) {
              onChange('');
              return;
            }
            onChange(next);
          }}
        >
          <SelectTrigger>
            <SelectValue placeholder={t('connections.providerDialog.remoteModelsPick')} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={CUSTOM_SUGGESTION}>
              {t('connections.providerDialog.remoteModelsCustom')}
            </SelectItem>
            {opts.map((id) => (
              <SelectItem key={id} value={id}>
                {id}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      ) : null}
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        readOnly={readOnly}
        placeholder={placeholder}
        autoComplete="off"
        spellCheck={false}
        className={className}
      />
      {statusLabel ? (
        statusRetry ? (
          <button
            type="button"
            className="self-start text-left text-meta text-accent hover:underline"
            onClick={statusRetry}
          >
            {statusLabel}
          </button>
        ) : (
          <p className="text-meta text-muted">{statusLabel}</p>
        )
      ) : null}
    </div>
  );
}

export function GenericConfigForm({
  schema,
  values,
  onChange,
  issues,
  disabled,
  className,
  readOnlyKeys,
  hiddenKeys,
  suggestions,
  fieldActions,
  fieldStatus,
  fieldHints,
}: GenericConfigFormProps) {
  const { t } = useI18n();
  const errMap = React.useMemo(() => issuesByField(issues), [issues]);
  const ro = React.useMemo(() => {
    if (!readOnlyKeys) return new Set<string>();
    return readOnlyKeys instanceof Set ? readOnlyKeys : new Set(readOnlyKeys);
  }, [readOnlyKeys]);
  const hidden = React.useMemo(() => {
    if (!hiddenKeys) return new Set<string>();
    return hiddenKeys instanceof Set ? hiddenKeys : new Set(hiddenKeys);
  }, [hiddenKeys]);

  const patch = (key: string, value: unknown) => {
    onChange({ ...values, [key]: value });
  };

  return (
    <div className={cn('flex flex-col gap-3', className)}>
      {schema.fields.map((field) => {
        if (hidden.has(field.key)) return null;
        const kind = fieldControlKind(field);
        const fieldDisabled = disabled || ro.has(field.key);
        const err = errMap[field.key];
        const raw = values[field.key];

        if (kind === 'unsupported') {
          return (
            <div
              key={field.key}
              className="rounded-card border border-border bg-canvas px-2.5 py-2 text-meta text-muted"
            >
              {t('connections.providerDialog.unsupportedField', { label: field.label })}
            </div>
          );
        }

        const visibleLabel = configFieldLabel(field.key, field.label, t);
        const extraHint = fieldHints?.[field.key]?.trim() || undefined;
        const hint = configFieldHint(field.key, extraHint, t);

        return (
          <React.Fragment key={field.key}>
            <label className="flex flex-col gap-1.5">
              <Hint label={hint}>
                <span className="text-xs text-muted">
                  {visibleLabel}
                  {field.required ? <span className="text-danger"> *</span> : null}
                </span>
              </Hint>
              {kind === 'secret' ? (
                <SecretInput
                  value={typeof raw === 'string' ? raw : raw == null ? '' : String(raw)}
                  onChange={(v) => patch(field.key, v)}
                  disabled={fieldDisabled}
                  readOnly={fieldDisabled}
                  placeholder={
                    typeof raw === 'string' && raw === SECRET_REDACTED
                      ? t('connections.providerDialog.secretConfigured')
                      : t('connections.apiKeyDialog.key')
                  }
                />
              ) : null}
              {kind === 'string' ? (
                <SuggestableInput
                  value={typeof raw === 'string' ? raw : raw == null ? '' : String(raw)}
                  onChange={(v) => patch(field.key, v)}
                  suggestions={suggestions?.[field.key]}
                  disabled={fieldDisabled}
                  readOnly={fieldDisabled}
                  className={fieldDisabled ? 'cursor-default bg-canvas text-secondary' : undefined}
                  statusLabel={fieldStatus?.[field.key]?.label}
                  statusRetry={fieldStatus?.[field.key]?.onRetry}
                />
              ) : null}
              {kind === 'number' ? (
                <Input
                  type="number"
                  value={raw == null || raw === '' ? '' : String(raw)}
                  onChange={(e) => {
                    const t = e.target.value;
                    patch(field.key, t === '' ? '' : Number(t));
                  }}
                  disabled={fieldDisabled}
                  autoComplete="off"
                />
              ) : null}
              {kind === 'boolean' ? (
                <div className="flex items-center gap-2">
                  <Switch
                    checked={Boolean(raw)}
                    onCheckedChange={(v) => patch(field.key, v)}
                    disabled={fieldDisabled}
                  />
                  <span className="text-meta text-muted">{hint ?? ''}</span>
                </div>
              ) : null}
              {kind === 'enum' && field.valueType.kind === 'enum' ? (
                <Select
                  value={typeof raw === 'string' && raw ? raw : field.valueType.options[0] ?? ''}
                  onValueChange={(v) => patch(field.key, v)}
                  disabled={fieldDisabled}
                >
                  <SelectTrigger>
                    <SelectValue placeholder={field.label} />
                  </SelectTrigger>
                  <SelectContent>
                    {field.valueType.options.map((opt) => (
                      <SelectItem key={opt} value={opt}>
                        {configFieldOptionLabel(field.key, opt, t)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              ) : null}
              {hint && kind !== 'boolean' ? (
                <span className="text-meta text-muted">{hint}</span>
              ) : null}
              {err ? <span className="text-meta text-danger">{err}</span> : null}
            </label>
            {fieldActions?.[field.key] ?? null}
          </React.Fragment>
        );
      })}
    </div>
  );
}
