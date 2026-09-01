import { useEffect, useMemo, useRef, useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { SecretInput } from '@/components/shared/SecretInput';
import { RouteEndpointTypeText } from '@/components/shared/RouteEndpointUrl';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useToast } from '@/components/ui/toast';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  getAgentConfigSchema,
  materializeAgentConfig,
  validateAgentConfig,
} from '@/lib/api/config';
import { detectApiEndpointTypes, upsertProvider } from '@/lib/api/provider';
import { applyFormVars } from '@/lib/provider-detect/fields';
import type { AgentId } from '@/lib/types';
import { attachPoolOwnedAuthorization } from '@/lib/api/adapter';
import {
  localEndpointBrandAgentId,
  localEndpointKindFromPool,
} from '@/lib/route-endpoints';
import { cn } from '@/lib/utils';
import {
  API_VENDORS,
  buildPoolApiSaveItems,
  defaultSelectedApiTypes,
  detectedApiChoiceTypes,
  poolApiChoices,
  primaryVendorUrl,
  resolveEndpointUrl,
  vendorServiceUrls,
  type PoolApiChoice,
  type PoolApiChoiceType,
} from './api-access-model';
import { savePoolApiAccess } from './save-pool-api-access';

const CUSTOM_VENDOR = 'custom';
const DETECT_DEBOUNCE_MS = 400;

const apiLabelKeys = {
  claudeMessages: 'routes.pool.page.apiClaude',
  openaiResponses: 'routes.pool.page.apiCodex',
  grokResponses: 'routes.pool.page.apiGrok',
  openaiChatCompletions: 'routes.pool.page.apiOpenaiChatCompletions',
} as const;

function endpointIdOf(endpoint: PoolApiChoice['endpoint']) {
  if (endpoint === '/v1/messages') return 'messages' as const;
  if (endpoint === '/v1/chat/completions') return 'chat_completions' as const;
  return 'responses' as const;
}

function brandAgentIdOf(choice: PoolApiChoice) {
  const kind = localEndpointKindFromPool({
    surface: endpointIdOf(choice.endpoint),
    targetAgentId: choice.agentId,
  });
  return kind ? localEndpointBrandAgentId(kind) : undefined;
}

export function ApiAccessDialog({
  open,
  agents,
  onOpenChange,
  onSaved,
}: {
  open: boolean;
  agents: readonly AgentId[];
  onOpenChange: (open: boolean) => void;
  onSaved?: () => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {open ? (
        <ApiAccessForm agents={agents} onOpenChange={onOpenChange} onSaved={onSaved} />
      ) : null}
    </Dialog>
  );
}

function ApiAccessForm({
  agents,
  onOpenChange,
  onSaved,
}: {
  agents: readonly AgentId[];
  onOpenChange: (open: boolean) => void;
  onSaved?: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [vendorId, setVendorId] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [selectedTypes, setSelectedTypes] = useState<Set<PoolApiChoiceType>>(new Set());
  const [detecting, setDetecting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detectNone, setDetectNone] = useState(false);
  const choices = useMemo(() => poolApiChoices(agents), [agents]);
  const selectedVendor = useMemo(
    () => API_VENDORS.find((item) => item.id === vendorId) ?? null,
    [vendorId],
  );
  const vendorUrls = selectedVendor ? vendorServiceUrls(selectedVendor) : [];
  const isUnknownVendor = vendorId === CUSTOM_VENDOR;
  const detectSeq = useRef(0);
  const choicesRef = useRef(choices);
  choicesRef.current = choices;

  const selectVendor = (nextVendorId: string) => {
    setVendorId(nextVendorId);
    setError(null);
    setDetectNone(false);
    if (nextVendorId === CUSTOM_VENDOR) {
      setSelectedTypes(new Set());
      return;
    }
    const vendor = API_VENDORS.find((item) => item.id === nextVendorId);
    if (!vendor) return;
    setBaseUrl(primaryVendorUrl(vendor));
    setSelectedTypes(defaultSelectedApiTypes(vendor, choices));
  };

  const toggleType = (type: PoolApiChoiceType, available: boolean) => {
    if (!available) return;
    setSelectedTypes((current) => {
      const next = new Set(current);
      if (next.has(type)) next.delete(type);
      else next.add(type);
      return next;
    });
  };

  useEffect(() => {
    if (!isUnknownVendor || saving) return;
    const url = baseUrl.trim();
    const key = apiKey.trim();
    if (!url || !key) return;
    const seq = ++detectSeq.current;
    const timer = window.setTimeout(() => {
      void (async () => {
        setDetecting(true);
        setError(null);
        setDetectNone(false);
        try {
          const detected = await detectApiEndpointTypes(url, key);
          if (seq !== detectSeq.current) return;
          const types = detectedApiChoiceTypes(detected).filter((type) =>
            choicesRef.current.some((choice) => choice.type === type && choice.available),
          );
          if (types.length === 0) {
            setDetectNone(true);
            return;
          }
          setSelectedTypes(new Set(types));
        } catch (cause) {
          if (seq !== detectSeq.current) return;
          setError(cause instanceof Error ? cause.message : String(cause));
        } finally {
          if (seq === detectSeq.current) setDetecting(false);
        }
      })();
    }, DETECT_DEBOUNCE_MS);
    return () => {
      window.clearTimeout(timer);
      detectSeq.current += 1;
      setDetecting(false);
    };
  }, [isUnknownVendor, baseUrl, apiKey, saving]);

  const save = async () => {
    const items = buildPoolApiSaveItems(choices, selectedTypes, selectedVendor, baseUrl);
    if (items.length === 0) return;
    setSaving(true);
    setError(null);
    try {
      const result = await savePoolApiAccess(
        { items, apiKey: apiKey.trim() },
        {
          getAgentConfigSchema,
          validateAgentConfig,
          materializeAgentConfig,
          applyFormVars,
          upsertProvider,
          attachAuthorization: async (sourceKind, sourceId, targetAgentId, surface) => {
            await attachPoolOwnedAuthorization({
              sourceKind,
              sourceId,
              targetAgentId,
              surface,
            });
          },
        },
      );
      if (result.saved === 0) {
        setError(result.errors[0] ?? t('routes.pool.page.addFailed'));
        toast({
          title: t('routes.pool.page.addFailed'),
          description: result.errors[0],
          variant: 'danger',
        });
        return;
      }
      toast({
        title: t('routes.pool.page.apiSavedCount', { count: result.saved }),
        description: result.errors[0],
        variant: 'success',
      });
      onSaved?.();
      onOpenChange(false);
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setError(message);
      toast({
        title: t('routes.pool.page.addFailed'),
        description: message,
        variant: 'danger',
      });
    } finally {
      setSaving(false);
    }
  };

  const canSave =
    Boolean(vendorId && baseUrl.trim() && apiKey.trim() && selectedTypes.size > 0)
    && !saving
    && !detecting;

  return (
    <DialogContent className="max-h-[85vh] max-w-lg overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{t('routes.pool.page.apiDialogTitle')}</DialogTitle>
          <DialogDescription>{t('routes.pool.page.apiDialogDescription')}</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('routes.pool.page.apiVendors')}</span>
            <Select value={vendorId || undefined} onValueChange={selectVendor}>
              <SelectTrigger>
                <SelectValue placeholder={t('routes.pool.page.apiVendorsPlaceholder')} />
              </SelectTrigger>
              <SelectContent>
                {API_VENDORS.map((vendor) => (
                  <SelectItem key={vendor.id} value={vendor.id}>
                    {t(vendor.labelKey)}
                  </SelectItem>
                ))}
                <SelectItem value={CUSTOM_VENDOR}>{t('routes.pool.page.apiVendorCustom')}</SelectItem>
              </SelectContent>
            </Select>
          </label>
          {selectedVendor ? (
            <p className="text-meta text-muted">{t('routes.pool.page.apiMatchedVendor')}</p>
          ) : null}
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('connections.providerDialog.endpoint')}</span>
            {vendorUrls.length > 1 ? (
              <Select value={baseUrl || undefined} onValueChange={setBaseUrl} disabled={!vendorId}>
                <SelectTrigger>
                  <SelectValue placeholder={t('routes.pool.page.apiUrlPlaceholder')} />
                </SelectTrigger>
                <SelectContent>
                  {vendorUrls.map((url) => (
                    <SelectItem key={url} value={url}>
                      {url}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <Input
                value={baseUrl}
                onChange={(event) => {
                  setBaseUrl(event.target.value);
                  setDetectNone(false);
                }}
                placeholder="https://api.example.com/v1"
                autoComplete="off"
                spellCheck={false}
                disabled={!vendorId}
              />
            )}
          </label>
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('connections.apiKeyDialog.key')}</span>
            <SecretInput value={apiKey} onChange={setApiKey} disabled={!vendorId} />
          </label>
          {isUnknownVendor ? (
            <p className="text-meta text-muted">
              {detecting ? t('routes.pool.page.apiDetecting') : t('routes.pool.page.apiDetectHint')}
            </p>
          ) : null}
          {detectNone ? <p className="text-meta text-muted">{t('routes.pool.page.apiDetectNone')}</p> : null}
          {error ? <p className="text-meta text-danger">{error}</p> : null}
          <div className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('routes.pool.page.apiTypesLabel')}</span>
            <div className="grid grid-cols-1 gap-2">
              {choices.map((choice) => {
                const checked = selectedTypes.has(choice.type);
                const actualUrl = resolveEndpointUrl(selectedVendor, choice.type, baseUrl);
                return (
                  <label
                    key={`${choice.agentId}-${choice.endpoint}-${choice.type}`}
                    className={cn(
                      'rounded-card border border-border bg-panel p-3',
                      choice.available ? 'cursor-pointer hover:bg-hover/50' : 'cursor-not-allowed opacity-60',
                    )}
                  >
                    <span className="flex items-start gap-3">
                      <input
                        type="checkbox"
                        className="mt-1"
                        checked={checked}
                        disabled={!choice.available || saving}
                        onChange={() => toggleType(choice.type, choice.available)}
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block font-mono text-sm font-medium text-primary">{choice.endpoint}</span>
                        <span className="block text-xs">
                          <RouteEndpointTypeText
                            endpointId={endpointIdOf(choice.endpoint)}
                            brandAgentId={brandAgentIdOf(choice)}
                          >
                            {t(apiLabelKeys[choice.type])}
                          </RouteEndpointTypeText>
                        </span>
                        {actualUrl ? (
                          <span className="mt-1 block truncate font-mono text-meta text-muted" title={actualUrl}>
                            {actualUrl}
                          </span>
                        ) : null}
                        {!choice.available ? (
                          <span className="mt-1 block text-xs text-muted">
                            {t('routes.pool.page.choiceUnavailable')}
                          </span>
                        ) : null}
                      </span>
                    </span>
                  </label>
                );
              })}
            </div>
          </div>
        </div>
        <DialogFooter>
          <Button type="button" variant="secondary" onClick={() => onOpenChange(false)} disabled={saving}>
            {t('common.cancel')}
          </Button>
          <Button type="button" onClick={() => void save()} disabled={!canSave}>
            {saving ? t('common.saving') : t('common.save')}
          </Button>
        </DialogFooter>
    </DialogContent>
  );
}
