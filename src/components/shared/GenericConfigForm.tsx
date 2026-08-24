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
              字段 <code className="font-mono">{field.key}</code>（{field.label}）类型不受支持
            </div>
          );
        }

        const visibleLabel =
          field.key === 'baseUrl'
            ? t('connections.providerDialog.endpoint')
            : field.key === 'apiKey'
              ? t('connections.apiKeyDialog.key')
              : field.key === 'model'
                ? t('connections.providerDialog.model')
                : field.label;
        const hint = field.help?.trim() || undefined;

        return (
          <label key={field.key} className="flex flex-col gap-1.5">
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
                placeholder={
                  typeof raw === 'string' && raw === SECRET_REDACTED
                    ? '已配置（留空保留）'
                    : 'API Key'
                }
              />
            ) : null}
            {kind === 'string' ? (
              <Input
                value={typeof raw === 'string' ? raw : raw == null ? '' : String(raw)}
                onChange={(e) => patch(field.key, e.target.value)}
                disabled={fieldDisabled}
                readOnly={fieldDisabled}
                autoComplete="off"
                spellCheck={false}
                className={fieldDisabled ? 'cursor-default bg-canvas text-secondary' : undefined}
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
                <span className="text-meta text-muted">{field.help ?? ''}</span>
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
                      {opt}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : null}
            {field.help && kind !== 'boolean' ? (
              <span className="text-meta text-muted">{field.help}</span>
            ) : null}
            {err ? <span className="text-meta text-danger">{err}</span> : null}
          </label>
        );
      })}
    </div>
  );
}
