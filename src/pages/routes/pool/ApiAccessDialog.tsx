import { useEffect, useMemo, useRef, useState } from 'react';
import { Plus, RefreshCw, Trash2 } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
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
import { detectApiEndpointTypes, listRemoteOpenAiModels, upsertProvider } from '@/lib/api/provider';
import { applyFormVars } from '@/lib/provider-detect/fields';
import type { AgentId } from '@/lib/types';
import {
  attachPoolOwnedAuthorization,
  setRouteAuthorizationPriority,
  setSourceCustomModels,
} from '@/lib/api/adapter';
import {
  localEndpointBrandAgentId,
  localEndpointKindFromPool,
} from '@/lib/route-endpoints';
import { cn } from '@/lib/utils';
import {
  API_VENDORS,
  CUSTOM_VENDOR_ID,
  buildPoolApiSaveItems,
  defaultSelectedApiTypes,
  detectedApiChoiceTypes,
  filterModelsByExclusions,
  mergeFetchedModels,
  parseApiKeyLines,
  parseExcludedModelRules,
  parsePriorityInput,
  poolApiChoices,
  primaryVendorUrl,
  resolveEndpointUrl,
  sortApiVendorsForPicker,
  vendorServiceUrls,
  type PoolApiChoice,
  type PoolApiChoiceType,
} from './api-access-model';
import { parseCustomModelList } from './pool-authorization-detail';
import { savePoolApiAccess } from './save-pool-api-access';

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
  const { t, lang } = useI18n();
  const { toast } = useToast();
  const [vendorId, setVendorId] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [selectedTypes, setSelectedTypes] = useState<Set<PoolApiChoiceType>>(new Set());
  const [models, setModels] = useState<string[]>([]);
  const [customModel, setCustomModel] = useState('');
  const [excludedModels, setExcludedModels] = useState('');
  const [priority, setPriority] = useState('');
  const [fetchingModels, setFetchingModels] = useState(false);
  const [modelError, setModelError] = useState<string | null>(null);
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
  const isUnknownVendor = vendorId === CUSTOM_VENDOR_ID;
  const pickerVendors = useMemo(
    () => sortApiVendorsForPicker(API_VENDORS, (vendor) => t(vendor.labelKey), lang),
    [lang, t],
  );
  const detectSeq = useRef(0);
  const choicesRef = useRef(choices);
  choicesRef.current = choices;

  const selectVendor = (nextVendorId: string) => {
    setVendorId(nextVendorId);
    setError(null);
    setDetectNone(false);
    if (nextVendorId === CUSTOM_VENDOR_ID) {
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
    const key = parseApiKeyLines(apiKey)[0] ?? '';
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

  const apiKeys = parseApiKeyLines(apiKey);
  const firstApiKey = apiKeys[0] ?? '';

  const addCustomModel = () => {
    const next = parseCustomModelList(customModel);
    if (next.length === 0) return;
    setModels((current) => {
      const seen = new Set(current.map((model) => model.trim()).filter(Boolean));
      const merged = [...current];
      for (const model of next) {
        if (seen.has(model)) continue;
        seen.add(model);
        merged.push(model);
      }
      return merged;
    });
    setCustomModel('');
  };

  const fetchModels = async () => {
    const url = baseUrl.trim();
    if (!url || !firstApiKey) {
      setModelError(t('routes.pool.page.apiModelsNeedKey'));
      return;
    }
    setFetchingModels(true);
    setModelError(null);
    try {
      const fetched = await listRemoteOpenAiModels(url, firstApiKey);
      setModels((current) => mergeFetchedModels(
        current,
        fetched,
        parseExcludedModelRules(excludedModels),
      ));
      if (fetched.length === 0) setModelError(t('routes.pool.page.apiModelsEmpty'));
    } catch (cause) {
      setModelError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setFetchingModels(false);
    }
  };

  const save = async () => {
    const items = buildPoolApiSaveItems(choices, selectedTypes, selectedVendor, baseUrl);
    if (items.length === 0 || apiKeys.length === 0) return;
    setSaving(true);
    setError(null);
    try {
      const result = await savePoolApiAccess(
        {
          items,
          apiKeys,
          models: filterModelsByExclusions(models, parseExcludedModelRules(excludedModels)),
          priority: parsePriorityInput(priority),
        },
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
          setSourceCustomModels,
          setAuthorizationPriority: setRouteAuthorizationPriority,
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
    Boolean(vendorId && baseUrl.trim() && apiKeys.length > 0 && selectedTypes.size > 0)
    && !saving
    && !detecting
    && !fetchingModels;

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
                <SelectItem value={CUSTOM_VENDOR_ID}>{t('routes.pool.page.apiVendorCustom')}</SelectItem>
                {pickerVendors.map((vendor) => (
                  <SelectItem key={vendor.id} value={vendor.id}>
                    {t(vendor.labelKey)}
                  </SelectItem>
                ))}
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
            <span className="text-xs text-muted">{t('routes.pool.page.apiKeysLabel')}</span>
            <textarea
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
              placeholder={t('routes.pool.page.apiKeysPlaceholder')}
              rows={3}
              autoComplete="off"
              spellCheck={false}
              disabled={!vendorId}
              className="min-h-[4.5rem] w-full resize-y rounded-btn border border-border-strong bg-panel px-2.5 py-2 font-mono text-body text-primary placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent/60 disabled:opacity-50"
            />
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
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs text-muted">{t('routes.pool.detail.models')}</span>
              <Button
                type="button"
                size="sm"
                variant="secondary"
                disabled={!vendorId || saving || fetchingModels}
                onClick={() => void fetchModels()}
              >
                <RefreshCw className={cn('h-3.5 w-3.5', fetchingModels && 'animate-spin')} />
                {fetchingModels
                  ? t('routes.pool.page.apiModelsFetching')
                  : t('routes.pool.page.apiModelsFetch')}
              </Button>
            </div>
            {models.length > 0 ? (
              <div className="flex flex-col gap-1.5">
                {models.map((model) => (
                  <div key={model} className="flex items-center gap-2">
                    <span className="min-w-0 flex-1 truncate font-mono text-xs text-primary" title={model}>
                      {model}
                    </span>
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      disabled={saving}
                      aria-label={t('routes.pool.page.apiModelsRemove')}
                      onClick={() => setModels((current) => current.filter((item) => item !== model))}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                ))}
              </div>
            ) : (
              <p className="text-meta text-muted">{t('routes.pool.page.apiModelsEmpty')}</p>
            )}
            <div className="flex items-center gap-2">
              <Input
                value={customModel}
                onChange={(event) => setCustomModel(event.target.value)}
                placeholder={t('routes.pool.page.apiModelsAddPlaceholder')}
                disabled={!vendorId || saving}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') {
                    event.preventDefault();
                    addCustomModel();
                  }
                }}
              />
              <Button
                type="button"
                size="sm"
                variant="secondary"
                disabled={!vendorId || saving || !customModel.trim()}
                onClick={addCustomModel}
              >
                <Plus className="h-3.5 w-3.5" />
                {t('routes.pool.page.apiModelsAdd')}
              </Button>
            </div>
            {modelError ? <p className="text-meta text-danger">{modelError}</p> : null}
          </div>
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('routes.pool.detail.priority')}</span>
            <Input
              inputMode="numeric"
              value={priority}
              onChange={(event) => setPriority(event.target.value.replace(/\D/g, ''))}
              placeholder={t('routes.pool.page.apiPriorityPlaceholder')}
              disabled={!vendorId || saving}
            />
          </label>
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('routes.pool.page.apiExcludedModels')}</span>
            <textarea
              value={excludedModels}
              onChange={(event) => setExcludedModels(event.target.value)}
              placeholder={t('routes.pool.page.apiExcludedModelsPlaceholder')}
              rows={3}
              autoComplete="off"
              spellCheck={false}
              disabled={!vendorId || saving}
              className="min-h-[4.5rem] w-full resize-y rounded-btn border border-border-strong bg-panel px-2.5 py-2 font-mono text-body text-primary placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent/60 disabled:opacity-50"
            />
          </label>
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
