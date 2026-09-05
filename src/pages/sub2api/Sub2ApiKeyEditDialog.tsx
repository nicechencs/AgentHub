import * as React from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Progress } from '@/components/ui/progress';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import type { Sub2ApiGroup, Sub2ApiKey } from '@/lib/sub2api';
import { cn } from '@/lib/utils';
import {
  addDaysToDateTimeLocal,
  formFromKey,
  formatDateTimeLocal,
  formatUsdFixed,
  pickQuotaLimit,
  pickQuotaUsed,
  pickRateWindow,
  rateUsagePercent,
  rateUsageTone,
  type Sub2ApiKeyForm,
} from './sub2api-key-form';
import { formatGroupRate } from './sub2api-page-model';

const TEXTAREA_CLASS =
  'w-full rounded-btn border border-border-strong bg-panel px-2.5 py-1.5 font-mono text-xs text-primary placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent/60 disabled:opacity-50';

function FieldLabel({ children }: { children: React.ReactNode }) {
  return <span className="text-sm text-secondary">{children}</span>;
}

function HintText({ children }: { children: React.ReactNode }) {
  return <p className="text-meta text-muted">{children}</p>;
}

function UsdInput({
  value,
  onChange,
  placeholder,
  disabled,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  disabled?: boolean;
}) {
  return (
    <div className="relative">
      <span className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-sm text-muted">
        $
      </span>
      <Input
        type="number"
        step="0.01"
        min="0"
        className="pl-6"
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}

function RateUsage({
  keyRow,
  window,
}: {
  keyRow: Sub2ApiKey;
  window: '5h' | '1d' | '7d';
}) {
  const { limit, used } = pickRateWindow(keyRow, window);
  if (limit <= 0) return null;
  const tone = rateUsageTone(used, limit);
  return (
    <div className="mt-2 space-y-1">
      <div className="rounded-lg bg-subtle px-3 py-2 text-sm">
        <span
          className={cn(
            'font-medium',
            tone === 'over' && 'text-danger',
            tone === 'warn' && 'text-warning',
          )}
        >
          {formatUsdFixed(used, 4)}
        </span>
        <span className="mx-2 text-muted">/</span>
        <span className="text-secondary">{formatUsdFixed(limit, 2)}</span>
      </div>
      <Progress
        value={rateUsagePercent(used, limit)}
        indicatorClassName={cn(
          tone === 'over' && 'bg-danger',
          tone === 'warn' && 'bg-warning',
          tone === 'ok' && 'bg-success',
        )}
      />
    </div>
  );
}

export function Sub2ApiKeyEditDialog({
  keyRow,
  groups,
  busy = false,
  onClose,
  onSave,
  onResetQuota,
  onResetRateLimit,
}: {
  keyRow: Sub2ApiKey;
  groups: readonly Sub2ApiGroup[];
  busy?: boolean;
  onClose: () => void;
  onSave: (form: Sub2ApiKeyForm) => void;
  onResetQuota: () => void;
  onResetRateLimit: () => void;
}) {
  const { t } = useI18n();
  const [form, setForm] = React.useState(() => formFromKey(keyRow));
  const [confirm, setConfirm] = React.useState<null | 'quota' | 'rate'>(null);

  React.useEffect(() => {
    setForm(formFromKey(keyRow));
  }, [keyRow.id]);

  const patch = (partial: Partial<Sub2ApiKeyForm>) => {
    setForm((prev) => ({ ...prev, ...partial }));
  };

  const quotaLimit = pickQuotaLimit(keyRow);
  const quotaUsed = pickQuotaUsed(keyRow);
  const hasRateLimit =
    pickRateWindow(keyRow, '5h').limit > 0
    || pickRateWindow(keyRow, '1d').limit > 0
    || pickRateWindow(keyRow, '7d').limit > 0;

  const extend = (days: 7 | 30 | 90) => {
    patch({
      expirationPreset: String(days) as '7' | '30' | '90',
      expirationDate: addDaysToDateTimeLocal(days),
      enableExpiration: true,
    });
  };

  return (
    <>
      <Dialog
        open
        onOpenChange={(open) => {
          if (!open && confirm == null) onClose();
        }}
      >
        <DialogContent className="max-w-lg" data-sub2api-edit-form="">
          <DialogHeader>
            <DialogTitle>{t('routes.sub2api.editKeyTitle')}</DialogTitle>
          </DialogHeader>
          <div className="space-y-5">
            <label className="block space-y-1.5">
              <FieldLabel>{t('routes.sub2api.createKeyName')}</FieldLabel>
              <Input
                value={form.name}
                disabled={busy}
                onChange={(e) => patch({ name: e.target.value })}
              />
            </label>

            {groups.length > 0 ? (
              <label className="block space-y-1.5">
                <FieldLabel>{t('routes.sub2api.selectGroup')}</FieldLabel>
                <Select
                  value={form.groupId == null ? undefined : String(form.groupId)}
                  onValueChange={(value) => {
                    const n = Number(value);
                    patch({ groupId: Number.isFinite(n) ? n : null });
                  }}
                >
                  <SelectTrigger data-sub2api-edit-group="">
                    <SelectValue placeholder={t('routes.sub2api.selectGroup')} />
                  </SelectTrigger>
                  <SelectContent>
                    {groups.map((group) => {
                      const rateLabel =
                        typeof group.rate_multiplier === 'number'
                        && Number.isFinite(group.rate_multiplier)
                          ? formatGroupRate(group.rate_multiplier)
                          : null;
                      return (
                        <SelectItem key={group.id} value={String(group.id)}>
                          <span className="flex w-full items-center justify-between gap-4">
                            <span>{group.name}</span>
                            {rateLabel ? (
                              <span className="ml-auto text-meta text-muted">{rateLabel}</span>
                            ) : null}
                          </span>
                        </SelectItem>
                      );
                    })}
                  </SelectContent>
                </Select>
              </label>
            ) : null}

            <label className="block space-y-1.5">
              <FieldLabel>{t('routes.sub2api.statusLabel')}</FieldLabel>
              <Select
                value={form.status}
                onValueChange={(value) => {
                  if (value === 'active' || value === 'inactive') patch({ status: value });
                }}
              >
                <SelectTrigger data-sub2api-edit-status="">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="active">{t('routes.sub2api.statusActive')}</SelectItem>
                  <SelectItem value="inactive">{t('routes.sub2api.statusDisabled')}</SelectItem>
                </SelectContent>
              </Select>
            </label>

            <div className="space-y-3">
              <div className="flex items-center justify-between gap-3">
                <FieldLabel>{t('routes.sub2api.ipRestriction')}</FieldLabel>
                <Switch
                  checked={form.enableIpRestriction}
                  disabled={busy}
                  onCheckedChange={(checked) => patch({ enableIpRestriction: checked })}
                />
              </div>
              {form.enableIpRestriction ? (
                <div className="space-y-3">
                  <label className="block space-y-1.5">
                    <FieldLabel>{t('routes.sub2api.ipWhitelist')}</FieldLabel>
                    <textarea
                      rows={3}
                      className={TEXTAREA_CLASS}
                      value={form.ipWhitelist}
                      disabled={busy}
                      placeholder={t('routes.sub2api.ipWhitelistPlaceholder')}
                      onChange={(e) => patch({ ipWhitelist: e.target.value })}
                    />
                    <HintText>{t('routes.sub2api.ipWhitelistHint')}</HintText>
                  </label>
                  <label className="block space-y-1.5">
                    <FieldLabel>{t('routes.sub2api.ipBlacklist')}</FieldLabel>
                    <textarea
                      rows={3}
                      className={TEXTAREA_CLASS}
                      value={form.ipBlacklist}
                      disabled={busy}
                      placeholder={t('routes.sub2api.ipBlacklistPlaceholder')}
                      onChange={(e) => patch({ ipBlacklist: e.target.value })}
                    />
                    <HintText>{t('routes.sub2api.ipBlacklistHint')}</HintText>
                  </label>
                </div>
              ) : null}
            </div>

            <div className="space-y-3">
              <FieldLabel>{t('routes.sub2api.quotaLimit')}</FieldLabel>
              <UsdInput
                value={form.quota}
                placeholder={t('routes.sub2api.quotaAmountPlaceholder')}
                disabled={busy}
                onChange={(quota) => patch({ quota })}
              />
              <HintText>{t('routes.sub2api.quotaAmountHint')}</HintText>
              {quotaLimit > 0 ? (
                <div className="space-y-1.5">
                  <FieldLabel>{t('routes.sub2api.quotaUsed')}</FieldLabel>
                  <div className="flex items-center gap-2">
                    <div className="flex-1 rounded-lg bg-subtle px-3 py-2 text-sm">
                      <span className="font-medium">{formatUsdFixed(quotaUsed, 4)}</span>
                      <span className="mx-2 text-muted">/</span>
                      <span className="text-secondary">{formatUsdFixed(quotaLimit, 2)}</span>
                    </div>
                    <Button
                      type="button"
                      variant="secondary"
                      size="sm"
                      disabled={busy}
                      title={t('routes.sub2api.resetQuotaUsed')}
                      onClick={() => setConfirm('quota')}
                    >
                      {t('routes.sub2api.reset')}
                    </Button>
                  </div>
                </div>
              ) : null}
            </div>

            <div className="space-y-3">
              <div className="flex items-center justify-between gap-3">
                <FieldLabel>{t('routes.sub2api.rateLimitSection')}</FieldLabel>
                <Switch
                  checked={form.enableRateLimit}
                  disabled={busy}
                  onCheckedChange={(checked) => patch({ enableRateLimit: checked })}
                />
              </div>
              {form.enableRateLimit ? (
                <div className="space-y-3">
                  <HintText>{t('routes.sub2api.rateLimitHint')}</HintText>
                  <label className="block space-y-1.5">
                    <FieldLabel>{t('routes.sub2api.rateLimit5h')}</FieldLabel>
                    <UsdInput
                      value={form.rateLimit5h}
                      placeholder="0"
                      disabled={busy}
                      onChange={(rateLimit5h) => patch({ rateLimit5h })}
                    />
                    <RateUsage keyRow={keyRow} window="5h" />
                  </label>
                  <label className="block space-y-1.5">
                    <FieldLabel>{t('routes.sub2api.rateLimit1d')}</FieldLabel>
                    <UsdInput
                      value={form.rateLimit1d}
                      placeholder="0"
                      disabled={busy}
                      onChange={(rateLimit1d) => patch({ rateLimit1d })}
                    />
                    <RateUsage keyRow={keyRow} window="1d" />
                  </label>
                  <label className="block space-y-1.5">
                    <FieldLabel>{t('routes.sub2api.rateLimit7d')}</FieldLabel>
                    <UsdInput
                      value={form.rateLimit7d}
                      placeholder="0"
                      disabled={busy}
                      onChange={(rateLimit7d) => patch({ rateLimit7d })}
                    />
                    <RateUsage keyRow={keyRow} window="7d" />
                  </label>
                  {hasRateLimit ? (
                    <Button
                      type="button"
                      variant="secondary"
                      size="sm"
                      disabled={busy}
                      onClick={() => setConfirm('rate')}
                    >
                      {t('routes.sub2api.resetRateLimitUsage')}
                    </Button>
                  ) : null}
                </div>
              ) : null}
            </div>

            <div className="space-y-3">
              <div className="flex items-center justify-between gap-3">
                <FieldLabel>{t('routes.sub2api.expiration')}</FieldLabel>
                <Switch
                  checked={form.enableExpiration}
                  disabled={busy}
                  onCheckedChange={(checked) => patch({ enableExpiration: checked })}
                />
              </div>
              {form.enableExpiration ? (
                <div className="space-y-3">
                  <div className="flex flex-wrap gap-2">
                    {([7, 30, 90] as const).map((days) => (
                      <button
                        key={days}
                        type="button"
                        disabled={busy}
                        onClick={() => extend(days)}
                        className={cn(
                          'rounded-lg px-3 py-1.5 text-sm transition-colors',
                          form.expirationPreset === String(days)
                            ? 'bg-accent/15 text-accent'
                            : 'bg-subtle text-secondary hover:bg-hover',
                        )}
                      >
                        {t('routes.sub2api.extendDays', { days })}
                      </button>
                    ))}
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => patch({ expirationPreset: 'custom' })}
                      className={cn(
                        'rounded-lg px-3 py-1.5 text-sm transition-colors',
                        form.expirationPreset === 'custom'
                          ? 'bg-accent/15 text-accent'
                          : 'bg-subtle text-secondary hover:bg-hover',
                      )}
                    >
                      {t('routes.sub2api.customDate')}
                    </button>
                  </div>
                  <label className="block space-y-1.5">
                    <FieldLabel>{t('routes.sub2api.expirationDate')}</FieldLabel>
                    <Input
                      type="datetime-local"
                      value={form.expirationDate}
                      disabled={busy}
                      onChange={(e) =>
                        patch({ expirationDate: e.target.value, expirationPreset: 'custom' })
                      }
                    />
                    <HintText>{t('routes.sub2api.expirationDateHint')}</HintText>
                  </label>
                  {keyRow.expires_at ? (
                    <p className="text-sm text-secondary">
                      {t('routes.sub2api.currentExpiration')}:{' '}
                      <span className="font-medium text-primary">
                        {formatDateTimeLocal(keyRow.expires_at).replace('T', ' ')}
                      </span>
                    </p>
                  ) : null}
                </div>
              ) : null}
            </div>
          </div>
          <DialogFooter className="gap-2 sm:gap-2">
            <Button type="button" variant="outline" onClick={onClose} disabled={busy}>
              {t('common.cancel')}
            </Button>
            <Button
              type="button"
              disabled={busy}
              onClick={() => onSave(form)}
            >
              {busy ? t('routes.sub2api.saving') : t('routes.sub2api.updateKey')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={confirm != null}
        onOpenChange={(open) => {
          if (!open) setConfirm(null);
        }}
      >
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>
              {confirm === 'rate'
                ? t('routes.sub2api.resetRateLimitTitle')
                : t('routes.sub2api.resetQuotaTitle')}
            </DialogTitle>
          </DialogHeader>
          <p className="text-sm text-secondary">
            {confirm === 'rate'
              ? t('routes.sub2api.resetRateLimitConfirm', { name: keyRow.name || `Key #${keyRow.id}` })
              : t('routes.sub2api.resetQuotaConfirm', {
                  name: keyRow.name || `Key #${keyRow.id}`,
                  used: formatUsdFixed(quotaUsed, 4),
                })}
          </p>
          <DialogFooter className="gap-2 sm:gap-2">
            <Button type="button" variant="outline" onClick={() => setConfirm(null)} disabled={busy}>
              {t('common.cancel')}
            </Button>
            <Button
              type="button"
              variant="danger"
              disabled={busy}
              onClick={() => {
                const kind = confirm;
                setConfirm(null);
                if (kind === 'rate') onResetRateLimit();
                else onResetQuota();
              }}
            >
              {t('routes.sub2api.reset')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
