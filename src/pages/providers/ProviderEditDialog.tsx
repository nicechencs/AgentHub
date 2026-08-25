/**
 * API Key 设置弹窗（产品层已合并原「供应商」）。
 * - 默认勾选「官方」：带出官方 URL / 模型
 * - 取消勾选：自定义端点与模型
 * - 配置保存：Catalog 声明有 Projector 时 fail-closed（validate/materialize）；
 *   仅 configSchemaVersion === null 时走 legacy applyFormVars。
 */
import * as React from 'react';
import { ChevronDown, FolderOpen, Sparkles } from 'lucide-react';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { ConfigEditor } from '@/components/shared/ConfigEditor';
import { GenericConfigForm, SuggestableInput } from '@/components/shared/GenericConfigForm';
import { useI18n } from '@/components/shared/LanguageProvider';
import { SecretInput } from '@/components/shared/SecretInput';
import { Hint, Tip } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import type { TranslateFn } from '@/lib/i18n';
import { useAgentCatalogOptional } from '@/app/runtime';
import { agentDisplayName } from '@/config/agents';
import {
  agentHasOfficialApiTemplate,
  looksLikeOfficialEndpoint,
  officialApiDefaults,
} from '@/config/official-api';
import {
  isPiAuthJsonSlot,
  isPiPlaceholderBaseUrl,
  PI_PLACEHOLDER_BASE_URL,
  PI_PROVIDER_SLOT_OPTIONS,
  piFormRequiresBaseUrl,
  piProviderSlotHint,
} from '@/lib/pi-provider-slots';
import {
  getAgentConfigSchema,
  materializeAgentConfig,
  validateAgentConfig,
  type AgentConfigSchemaDto,
} from '@/lib/api/config';
import { openAgentConfigDir } from '@/lib/api/install';
import {
  listRemoteOpenAiModels,
  listRemoteOpenAiModelsForProvider,
  upsertProvider,
} from '@/lib/api/provider';
import type { AgentId, Provider } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  applyFormVars,
  applySmartPaste,
  defaultConfigScaffold,
  EMPTY_FORM_VARS,
  extractFormVars,
  formFieldVisibility,
  initFormFromConfig,
  isLiveFilePath,
  isLivePastedApiKey,
  liveConfigPaths,
  REDACTED_MARKER,
  remoteModelsStatusView,
  resolveUpstreamBaseUrl,
  shouldFetchRemoteModels,
  smartDetectUrlAndKey,
  validateNativeConfigText,
  nativeConfigIssueMessage,
  withDefaultModel,
  type NativeConfigIssue,
  type ProviderFormVars,
} from '@/lib/provider-detect';
import {
  officialToggleNext,
  type OfficialToggleForm,
  type OfficialToggleSnapshot,
} from './official-toggle';
import {
  canSaveProviderForm,
  canSaveWithSchemaStatus,
  planSchemaLoad,
  resolveProjectorExpectation,
  runProviderSaveFlow,
  schemaErrorMessage,
  type SchemaUiStatus,
} from './providerSaveFlow';

const REMOTE_MODELS_DEBOUNCE_MS = 400;

export type ProviderDialogMode = 'add' | 'edit';

function officialFormState(agentId: AgentId, keepKey: string): OfficialToggleForm | null {
  const off = officialApiDefaults(agentId);
  if (!off) return null;
  const extracted = extractFormVars(agentId, off.scaffoldText, off.format);
  const vars: ProviderFormVars = {
    ...extracted,
    baseUrl: off.baseUrl,
    model: off.model,
    modelOpus: off.modelOpus ?? extracted.modelOpus,
    modelSonnet: off.modelSonnet ?? extracted.modelSonnet,
    modelHaiku: off.modelHaiku ?? extracted.modelHaiku,
    modelFable: off.modelFable ?? extracted.modelFable,
    modelSubagent: off.modelSubagent ?? extracted.modelSubagent,
    apiKey: keepKey,
  };
  return {
    vars,
    configFormat: off.format,
    configText: applyFormVars(agentId, off.scaffoldText, off.format, {
      ...extracted,
      baseUrl: off.baseUrl,
      model: off.model,
      modelOpus: off.modelOpus ?? '',
      modelSonnet: off.modelSonnet ?? '',
      modelHaiku: off.modelHaiku ?? '',
      modelFable: off.modelFable ?? '',
      modelSubagent: off.modelSubagent ?? '',
      apiKey: keepKey,
    }),
  };
}

function piSlotSelectOptions(slug: string) {
  const id = slug.trim() || 'custom';
  if (PI_PROVIDER_SLOT_OPTIONS.some((slot) => slot.id === id)) {
    return PI_PROVIDER_SLOT_OPTIONS;
  }
  return [...PI_PROVIDER_SLOT_OPTIONS, { id, label: id }];
}

function translateNativeConfigIssue(issue: NativeConfigIssue, t: TranslateFn): string {
  switch (issue.code) {
    case 'json_must_be_object':
      return t('connections.providerDialog.configMustBeObject');
    case 'json_parse':
      return t('connections.providerDialog.configParseFailed', { detail: issue.detail ?? '' });
    case 'toml_parse':
      return t('connections.providerDialog.configTomlParseFailed', { detail: issue.detail ?? '' });
    case 'expect_toml':
      return t('connections.providerDialog.configExpectToml');
    case 'claude_env_object':
      return t('connections.providerDialog.configClaudeEnvObject');
    case 'claude_env_string':
      return t('connections.providerDialog.configClaudeEnvString', { key: issue.detail ?? '' });
    case 'claude_foreign_keys':
      return t('connections.providerDialog.configClaudeForeignKeys', {
        keys: issue.keys?.join('、') ?? '',
      });
  }
}

export function getConfigTextError(
  agentId: AgentId,
  configText: string,
  configFormat: 'json' | 'toml',
  t?: TranslateFn,
): string | null {
  const issue = validateNativeConfigText(agentId, configText, configFormat);
  if (!issue) return null;
  if (!t) return nativeConfigIssueMessage(issue);
  return translateNativeConfigIssue(issue, t);
}

export function ProviderEditDialog({
  agentId,
  open,
  onOpenChange,
  mode = 'add',
  provider,
  onSaved,
  asPanel = false,
  width,
}: {
  agentId: AgentId;
  open: boolean;
  onOpenChange: (v: boolean) => void;
  mode?: ProviderDialogMode;
  provider?: Provider | null;
  onSaved: (p: Provider) => void;
  asPanel?: boolean;
  width?: number;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const catalog = useAgentCatalogOptional();
  const isEdit = mode === 'edit';
  const agentName = agentDisplayName(agentId);
  const livePaths = liveConfigPaths(agentId);

  const [name, setName] = React.useState('');
  const [configText, setConfigText] = React.useState('');
  const [configFormat, setConfigFormat] = React.useState<'json' | 'toml'>('json');
  const [configError, setConfigError] = React.useState<string | null>(null);
  const [vars, setVars] = React.useState<ProviderFormVars>({ ...EMPTY_FORM_VARS });
  const [pasteBuf, setPasteBuf] = React.useState('');
  const [detectHints, setDetectHints] = React.useState<string[]>([]);
  const [saving, setSaving] = React.useState(false);
  /** 默认官方：带出官方 URL / 模型 */
  const [useOfficial, setUseOfficial] = React.useState(true);
  /** Custom URL / model / advanced config to restore after unchecking official. */
  const [customSnapshot, setCustomSnapshot] = React.useState<OfficialToggleSnapshot | null>(
    null,
  );
  const [showAdvanced, setShowAdvanced] = React.useState(true);
  /** Backend config schema when Catalog declares a projector. */
  const [configSchema, setConfigSchema] = React.useState<AgentConfigSchemaDto | null>(
    null,
  );
  const [schemaStatus, setSchemaStatus] = React.useState<SchemaUiStatus>('idle');
  const [schemaError, setSchemaError] = React.useState<string | null>(null);
  /** Bump to re-run schema load without clearing form fields. */
  const [schemaLoadToken, setSchemaLoadToken] = React.useState(0);
  const [remoteModels, setRemoteModels] = React.useState<string[]>([]);
  const [remoteModelsError, setRemoteModelsError] = React.useState(false);
  const [remoteModelsLoading, setRemoteModelsLoading] = React.useState(false);
  const [remoteModelsRetry, setRemoteModelsRetry] = React.useState(0);
  const remoteModelsSeq = React.useRef(0);

  const official = officialApiDefaults(agentId);

  const catalogEntry = React.useMemo(
    () => catalog.entries.find((e) => e.key === agentId),
    [catalog.entries, agentId],
  );

  React.useEffect(() => {
    if (!open) return;
    let cancelled = false;

    const expectation = resolveProjectorExpectation({
      catalogStatus: catalog.status,
      entry: catalogEntry,
    });
    const plan = planSchemaLoad(expectation);

    if (plan.action === 'wait') {
      setSchemaStatus('loading');
      setSchemaError(null);
      setConfigSchema(null);
      return;
    }
    if (plan.action === 'unsupported') {
      setSchemaStatus('unsupported');
      setSchemaError(null);
      setConfigSchema(null);
      return;
    }
    if (plan.action === 'error') {
      setSchemaStatus('error');
      setSchemaError(plan.message);
      setConfigSchema(null);
      return;
    }

    // load_schema — Catalog requires projector
    setSchemaStatus('loading');
    setSchemaError(null);
    void getAgentConfigSchema(agentId)
      .then((schema) => {
        if (cancelled) return;
        setConfigSchema(schema);
        setSchemaStatus('ready');
        setSchemaError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setConfigSchema(null);
        setSchemaStatus('error');
        setSchemaError(
          e instanceof Error
            ? e.message || schemaErrorMessage('schema_load_failed')
            : schemaErrorMessage('schema_load_failed'),
        );
      });

    return () => {
      cancelled = true;
    };
  }, [open, agentId, catalog.status, catalogEntry, schemaLoadToken]);

  const retrySchemaLoad = () => {
    // Do not clear name / vars / configText — only re-evaluate Catalog + schema.
    setSchemaLoadToken((token) => token + 1);
  };

  const schemaReadOnlyKeys = React.useMemo(() => {
    if (!useOfficial) return undefined;
    return new Set([
      'baseUrl',
      'model',
      'modelOpus',
      'modelSonnet',
      'modelHaiku',
      'modelFable',
      'modelSubagent',
      'contextWindow',
      'reasoningEffort',
      'wireApi',
      'providerSlug',
    ]);
  }, [useOfficial]);

  const applyOfficialDefaults = React.useCallback(
    (keepKey?: string) => {
      const form = officialFormState(agentId, keepKey ?? '');
      if (!form) {
        const scaffold = defaultConfigScaffold(agentId);
        setConfigText(scaffold.text);
        setConfigFormat(scaffold.format);
        setConfigError(null);
        const extracted = extractFormVars(agentId, scaffold.text, scaffold.format);
        setVars({
          ...extracted,
          apiKey: keepKey ?? extracted.apiKey,
        });
        return;
      }
      setConfigFormat(form.configFormat);
      setConfigText(form.configText);
      setConfigError(null);
      setVars(form.vars);
    },
    [agentId],
  );

  React.useEffect(() => {
    if (!open) return;
    setPasteBuf('');
    setDetectHints([]);
    setCustomSnapshot(null);
    if (isEdit && provider) {
      setName(provider.name ?? '');
      setConfigText(provider.configText);
      setConfigFormat(provider.configFormat);
      setConfigError(getConfigTextError(agentId, provider.configText, provider.configFormat, t));
      const nextVars = initFormFromConfig(
        agentId,
        provider.configText,
        provider.configFormat,
        provider.authApiKey,
      );
      setVars(nextVars);
      const normalized = applyFormVars(
        agentId,
        provider.configText,
        provider.configFormat,
        nextVars,
      );
      const normalizedError = getConfigTextError(agentId, normalized, provider.configFormat, t);
      if (!normalizedError) {
        setConfigText(normalized);
        setConfigError(null);
      }
      const resolvedOnOpen = resolveUpstreamBaseUrl({
        formBaseUrl: nextVars.baseUrl,
        configText: provider.configText,
        configFormat: provider.configFormat,
        agentId,
      });
      const inferredOfficial =
        provider.official === true ||
        (provider.official !== false &&
          looksLikeOfficialEndpoint(agentId, nextVars.baseUrl || resolvedOnOpen));
      setUseOfficial(inferredOfficial);
      setShowAdvanced(true);
      return;
    }
    // 新增：有官方模板才默认官方（Pi 无单一官方 URL）
    setName('');
    setConfigError(null);
    setUseOfficial(agentHasOfficialApiTemplate(agentId));
    setShowAdvanced(true);
    applyOfficialDefaults();
  }, [open, isEdit, provider, agentId, applyOfficialDefaults, t]);

  const onToggleOfficial = (checked: boolean) => {
    const next = officialToggleNext({
      checked,
      current: { vars, configText, configFormat },
      snapshot: customSnapshot,
      official: checked ? officialFormState(agentId, vars.apiKey) : null,
    });
    setCustomSnapshot(next.snapshot);
    setUseOfficial(checked);
    setVars(next.vars);
    setConfigText(next.configText);
    setConfigFormat(next.configFormat);
    setConfigError(getConfigTextError(agentId, next.configText, next.configFormat, t));
    if (checked && !name.trim() && official) {
      setName(official.label);
    }
  };

  const tomlOpaque =
    configFormat === 'toml' && configText.trim() === REDACTED_MARKER;

  const runSmartPaste = React.useCallback(
    (raw: string, opts?: { fillName?: boolean }) => {
      const result = applySmartPaste(agentId, raw, {
        configText,
        configFormat,
        vars,
      });
      setDetectHints(result.detect.hints);
      setVars(result.vars);
      setConfigText(result.configText);
      setConfigFormat(result.configFormat);
      setConfigError(getConfigTextError(agentId, result.configText, result.configFormat, t));
      if (opts?.fillName !== false && result.suggestedName) {
        setName((n) => n.trim() || result.suggestedName || '');
      }
      return result;
    },
    [agentId, configText, configFormat, vars, t],
  );

  const patchVars = (patch: Partial<ProviderFormVars>) => {
    setVars((prev) => {
      const next = { ...prev, ...patch };
      const base =
        configText.trim() === REDACTED_MARKER || !configText.trim()
          ? defaultConfigScaffold(agentId).text
          : configText;
      const nextConfigText = applyFormVars(agentId, base, configFormat, next);
      setConfigText(nextConfigText);
      setConfigError(getConfigTextError(agentId, nextConfigText, configFormat, t));
      return next;
    });
  };

  const onSmartPaste = () => {
    const result = runSmartPaste(pasteBuf, { fillName: true });
    if (result.detect.baseUrl || result.detect.apiKey) {
      toast({
        title: t('connections.providerDialog.recognized'),
        description: result.detect.hints.join(' · '),
        variant: 'success',
      });
    } else {
      toast({
        title: t('connections.providerDialog.notRecognized'),
        description: t('connections.providerDialog.notRecognizedDesc'),
        variant: 'danger',
      });
    }
  };

  const onFieldPaste = (
    field: 'baseUrl' | 'apiKey',
    e: React.ClipboardEvent<HTMLInputElement>,
  ) => {
    const text = e.clipboardData.getData('text');
    if (!text || text.length < 8) return;
    const singleLine = !text.includes('\n') && text.trim().length < 400;
    if (singleLine && field === 'baseUrl' && /^https?:\/\//i.test(text.trim())) return;
    if (
      singleLine &&
      field === 'apiKey' &&
      !/^https?:\/\//i.test(text.trim()) &&
      !text.includes('=')
    ) {
      return;
    }
    e.preventDefault();
    setPasteBuf(text);
    runSmartPaste(text, { fillName: true });
  };

  const onConfigTextChange = (text: string) => {
    setConfigText(text);
    setConfigError(getConfigTextError(agentId, text, configFormat, t));
    const extracted = extractFormVars(agentId, text, configFormat);
    const hit = smartDetectUrlAndKey(text);
    setVars({
      ...extracted,
      baseUrl: extracted.baseUrl || hit.baseUrl || '',
      apiKey: extracted.apiKey || hit.apiKey || '',
      model: extracted.model || hit.model || '',
    });
  };

  const piSlug = vars.providerSlug.trim() || 'custom';
  const piNeedsUrl = agentId === 'pi' && piFormRequiresBaseUrl(piSlug);

  // 新增必须填 Key；编辑可只改名称/官方开关（Key 留空保留）
  // schema idle/loading/error → fail closed，禁止保存
  // model is optional; empty is filled on save via withDefaultModel
  const canSave = canSaveProviderForm({
    schemaStatus,
    configError,
    isEdit,
    apiKey: vars.apiKey,
    piNeedsUrl,
    baseUrl: vars.baseUrl,
    model: vars.model,
  });

  const hasStoredSecret = Boolean(provider?.id);
  const resolvedBaseUrl = resolveUpstreamBaseUrl({
    formBaseUrl: vars.baseUrl,
    configText,
    configFormat,
    agentId,
  });
  const shouldFetch = shouldFetchRemoteModels({
    useOfficial,
    baseUrl: resolvedBaseUrl,
    apiKey: vars.apiKey,
    hasStoredSecret,
  });
  const remoteStatus = remoteModelsStatusView({
    loading: remoteModelsLoading,
    error: remoteModelsError,
    ids: remoteModels,
    active: open && shouldFetch,
  });
  const retryRemoteModels = React.useCallback(() => {
    setRemoteModelsRetry((token) => token + 1);
  }, []);
  const modelFieldStatus =
    shouldFetch && remoteStatus.labelKey
      ? {
          label: t(remoteStatus.labelKey),
          onRetry: remoteStatus.showRetry ? retryRemoteModels : undefined,
        }
      : undefined;

  React.useEffect(() => {
    if (!open) {
      remoteModelsSeq.current += 1;
      setRemoteModels([]);
      setRemoteModelsError(false);
      setRemoteModelsLoading(false);
      return;
    }
    if (useOfficial || !shouldFetch) {
      remoteModelsSeq.current += 1;
      setRemoteModels([]);
      setRemoteModelsError(false);
      setRemoteModelsLoading(false);
      return;
    }
    const seq = ++remoteModelsSeq.current;
    setRemoteModelsLoading(true);
    setRemoteModelsError(false);
    const handle = window.setTimeout(() => {
      const request = isLivePastedApiKey(vars.apiKey)
        ? listRemoteOpenAiModels(resolvedBaseUrl, vars.apiKey)
        : provider?.id
          ? listRemoteOpenAiModelsForProvider(provider.id, resolvedBaseUrl)
          : Promise.reject(new Error('no stored secret'));
      void request
        .then((ids) => {
          if (seq !== remoteModelsSeq.current) return;
          setRemoteModels(ids);
          setRemoteModelsError(false);
        })
        .catch(() => {
          if (seq !== remoteModelsSeq.current) return;
          setRemoteModels([]);
          setRemoteModelsError(true);
        })
        .finally(() => {
          if (seq !== remoteModelsSeq.current) return;
          setRemoteModelsLoading(false);
        });
    }, REMOTE_MODELS_DEBOUNCE_MS);
    return () => {
      window.clearTimeout(handle);
    };
  }, [
    open,
    useOfficial,
    shouldFetch,
    resolvedBaseUrl,
    vars.apiKey,
    provider?.id,
    remoteModelsRetry,
    configText,
    configFormat,
  ]);

  const openLiveDir = async () => {
    try {
      const path = await openAgentConfigDir(agentId);
      toast({
        title: t('connections.providerDialog.openedConfigDir'),
        description: path,
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: t('connections.providerDialog.openConfigDirFailed'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    }
  };

  const requestClose = () => {
    if (saving) return;
    onOpenChange(false);
  };

  const save = async () => {
    if (configError) {
      toast({
        title: t('connections.providerDialog.invalidConfig'),
        description: t('connections.providerDialog.invalidConfigDesc', { error: configError }),
        variant: 'danger',
      });
      return;
    }
    if (!canSaveWithSchemaStatus(schemaStatus)) {
      toast({
        title: t('connections.providerDialog.cannotSave'),
        description: schemaError ?? t('connections.providerDialog.schemaNotReady'),
        variant: 'danger',
      });
      return;
    }
    setSaving(true);
    try {
      const off = officialApiDefaults(agentId);
      const scaffold =
        useOfficial && off
          ? { text: off.scaffoldText, format: off.format, preset: off.presetId }
          : defaultConfigScaffold(agentId);
      const baseText =
        configText.trim() === REDACTED_MARKER || !configText.trim()
          ? scaffold.text
          : configText;
      // 官方模式强制模型/URL 再写一遍，避免用户半改
      const saveVars: ProviderFormVars = withDefaultModel(
        agentId,
        useOfficial && off
          ? {
              ...vars,
              baseUrl: off.baseUrl,
              model: off.model,
              modelOpus: off.modelOpus ?? vars.modelOpus,
              modelSonnet: off.modelSonnet ?? vars.modelSonnet,
              modelHaiku: off.modelHaiku ?? vars.modelHaiku,
              modelFable: off.modelFable ?? vars.modelFable,
              modelSubagent: off.modelSubagent ?? vars.modelSubagent,
            }
          : vars,
        useOfficial,
      );
      const finalFormat = useOfficial && off ? off.format : configFormat;

      const result = await runProviderSaveFlow(
        {
          agentId,
          schemaStatus,
          configSchema,
          isEdit,
          existing: provider,
          name,
          useOfficial,
          officialLabel: off?.label,
          officialPresetId: off?.presetId,
          configText,
          configFormat,
          vars,
          saveVars,
          finalFormat,
          baseText,
        },
        {
          validateAgentConfig,
          materializeAgentConfig,
          applyFormVars,
          upsertProvider,
        },
      );

      if (!result.ok) {
        toast({
          title: isEdit ? t('connections.apiKeyDialog.updateFailed') : t('connections.apiKeyDialog.addFailed'),
          description: result.message,
          variant: 'danger',
        });
        // preserveInput: leave form state untouched
        return;
      }

      const endpointLabel =
        agentId === 'pi'
          ? isPiAuthJsonSlot(saveVars.providerSlug) && !saveVars.baseUrl.trim()
            ? t('connections.providerDialog.officialVendorSlot')
            : t('connections.list.customEndpoint')
          : useOfficial
            ? t('connections.list.officialEndpoint')
            : t('connections.list.customEndpoint');
      toast({
        title: isEdit ? t('connections.apiKeyDialog.updated') : t('connections.apiKeyDialog.added'),
        description: result.provider.isCurrent
          ? t('connections.providerDialog.wroteLocal', {
              name: result.provider.name,
              endpoint: endpointLabel,
            })
          : t('connections.providerDialog.savedPool', {
              name: result.provider.name,
              endpoint: endpointLabel,
            }),
        variant: 'success',
      });
      onSaved(result.provider);
      onOpenChange(false);
    } catch (e) {
      toast({
        title: isEdit ? t('connections.apiKeyDialog.updateFailed') : t('connections.apiKeyDialog.addFailed'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setSaving(false);
    }
  };

  const title = isEdit
    ? t('connections.apiKeyDialog.editTitle', { name: agentName })
    : t('connections.apiKeyDialog.addTitle', { name: agentName });
  const keyHint = isEdit ? t('connections.apiKeyDialog.keyHint') : undefined;
  const cancelButton = (
    <Button type="button" variant="secondary" size="sm" onClick={requestClose} disabled={saving}>
      {t('common.cancel')}
    </Button>
  );
  const saveButton = (
    <Button disabled={!canSave || saving} onClick={() => void save()} size="sm">
      {saving
        ? t('common.saving')
        : isEdit
          ? t('connections.apiKeyDialog.saveEdit')
          : t('connections.providerDialog.add')}
    </Button>
  );
  const headerActions = (
    <>
      {cancelButton}
      {saveButton}
    </>
  );

  const form = (
        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-card border border-border bg-canvas px-3 py-2 text-meta text-muted">
            <div className="min-w-0 flex-1">
              <Tip label={livePaths.hint}>
                <span className="block">
                  <span className="text-secondary">{t('connections.providerDialog.liveConfig')}</span>
                  <code className="break-all font-mono">{livePaths.config}</code>
                  {isLiveFilePath(livePaths.auth) ? (
                    <span className="mt-0.5 block">
                      <span className="text-secondary">{t('connections.providerDialog.liveAuth')}</span>
                      <code className="break-all font-mono">{livePaths.auth}</code>
                    </span>
                  ) : null}
                </span>
              </Tip>
            </div>
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="shrink-0"
              onClick={() => void openLiveDir()}
              title={t('connections.providerDialog.openDirTitle', { dir: livePaths.openDir })}
            >
              <FolderOpen className="h-3.5 w-3.5" /> {t('connections.providerDialog.openDir')}
            </Button>
          </div>

          {official ? (
            <Hint label={t('connections.providerDialog.useOfficialHint', { label: official.label })}>
              <label className="flex cursor-pointer items-start gap-2.5 rounded-card border border-border bg-panel px-3 py-2.5">
                <input
                  type="checkbox"
                  className="mt-0.5 h-4 w-4 accent-accent"
                  checked={useOfficial}
                  onChange={(e) => onToggleOfficial(e.target.checked)}
                />
                <span className="min-w-0 flex-1 text-sm font-medium text-primary">
                  {t('connections.providerDialog.useOfficial')}
                </span>
              </label>
            </Hint>
          ) : null}

          {/* 布局固定：勾选官方只切换只读/禁用，不卸载区块，避免高度跳动 */}
          <div
            className={cn(
              'flex flex-col gap-1.5 rounded-card border border-border bg-canvas p-3 transition-opacity',
              useOfficial && 'pointer-events-none opacity-45',
            )}
            aria-disabled={useOfficial || undefined}
          >
            <Hint label={t('connections.providerDialog.smartDetectHint')}>
              <span className="flex items-center gap-1 text-xs font-medium text-secondary">
                <Sparkles className="h-3.5 w-3.5" />
                {t('connections.providerDialog.smartDetect')}
              </span>
            </Hint>
            <textarea
              value={pasteBuf}
              onChange={(e) => {
                if (useOfficial) return;
                setPasteBuf(e.target.value);
              }}
              onPaste={(e) => {
                if (useOfficial) return;
                const text = e.clipboardData.getData('text');
                if (text && text.length > 12) {
                  window.setTimeout(() => {
                    setPasteBuf(text);
                    runSmartPaste(text, { fillName: true });
                  }, 0);
                }
              }}
              rows={3}
              disabled={useOfficial}
              placeholder={t('connections.providerDialog.pastePlaceholder')}
              className="w-full resize-none rounded-btn border border-border bg-panel px-2.5 py-2 font-mono text-xs text-primary placeholder:text-muted focus:outline-none focus:ring-2 focus:ring-accent/60 disabled:cursor-not-allowed"
              spellCheck={false}
            />
            <div className="flex min-h-[1.75rem] flex-wrap items-center gap-2">
              <Button
                type="button"
                size="sm"
                variant="secondary"
                disabled={useOfficial || !pasteBuf.trim()}
                onClick={onSmartPaste}
              >
                {t('connections.providerDialog.detectFill')}
              </Button>
              <span className="text-meta text-muted">
                {detectHints.length > 0 ? detectHints.join(' · ') : '\u00a0'}
              </span>
            </div>
          </div>

          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">{t('connections.apiKeyDialog.name')}</span>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={
                useOfficial && official ? official.label : t('connections.providerDialog.namePlaceholderCustom')
              }
              autoComplete="off"
            />
          </label>

          {/* 固定占位，避免 tomlOpaque 出现时把表单顶高 */}
          <p
            className={cn(
              'min-h-[2.25rem] rounded-card border px-2.5 py-2 text-meta',
              tomlOpaque
                ? 'border-border bg-canvas text-muted'
                : 'border-transparent text-transparent',
            )}
            aria-hidden={!tomlOpaque}
          >
            {tomlOpaque
              ? t('connections.providerDialog.tomlOpaque')
              : '\u00a0'}
          </p>

          {configError ? (
            <p
              role="alert"
              className="rounded-card border border-danger/40 bg-danger/5 px-2.5 py-2 text-meta text-danger"
            >
              {t('connections.providerDialog.configErrorKeep', { error: configError })}
            </p>
          ) : null}

          {schemaStatus === 'loading' || schemaStatus === 'idle' ? (
            <p className="text-meta text-muted">{t('connections.providerDialog.loadingSchema')}</p>
          ) : null}

          {schemaStatus === 'error' ? (
            <div className="flex flex-col gap-2 rounded-card border border-danger/40 bg-danger/5 px-3 py-2.5">
              <p className="text-meta text-danger">
                {schemaError ?? t('connections.providerDialog.schemaUnavailable')}
              </p>
              <Button
                type="button"
                size="sm"
                variant="outline"
                className="self-start"
                onClick={retrySchemaLoad}
              >
                {t('chrome.error.retry')}
              </Button>
            </div>
          ) : null}

          {configSchema && schemaStatus === 'ready' ? (
            <GenericConfigForm
              schema={configSchema}
              values={vars as unknown as Record<string, unknown>}
              onChange={(next) => {
                if (useOfficial) {
                  // Official mode: only allow secret edits
                  const key = String(next.apiKey ?? vars.apiKey);
                  if (key !== vars.apiKey) patchVars({ apiKey: key });
                  return;
                }
                patchVars(next as Partial<ProviderFormVars>);
              }}
              readOnlyKeys={schemaReadOnlyKeys}
              disabled={false}
              suggestions={
                !useOfficial && remoteStatus.showPicker ? { model: remoteModels } : undefined
              }
              fieldStatus={modelFieldStatus ? { model: modelFieldStatus } : undefined}
              fieldHints={keyHint ? { apiKey: keyHint } : undefined}
            />
          ) : schemaStatus === 'unsupported' ? (
            <>
              {formFieldVisibility(agentId).providerSlug ? (
                <label className="flex flex-col gap-1.5">
                  <span className="text-xs text-muted">{t('connections.providerDialog.fields.providerSlug')}</span>
                  <Select
                    value={vars.providerSlug.trim() || 'custom'}
                    onValueChange={(value) => {
                      const patch: Partial<ProviderFormVars> = { providerSlug: value };
                      if (isPiAuthJsonSlot(value) && isPiPlaceholderBaseUrl(vars.baseUrl)) {
                        patch.baseUrl = '';
                      } else if (
                        piFormRequiresBaseUrl(value) &&
                        !vars.baseUrl.trim()
                      ) {
                        patch.baseUrl = PI_PLACEHOLDER_BASE_URL;
                      }
                      patchVars(patch);
                    }}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder={t('connections.providerDialog.slotPlaceholder')} />
                    </SelectTrigger>
                    <SelectContent>
                      {piSlotSelectOptions(vars.providerSlug).map((slot) => (
                        <SelectItem key={slot.id} value={slot.id}>
                          {slot.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <span className="text-meta text-muted">{piProviderSlotHint(piSlug)}</span>
                </label>
              ) : null}
              <label className="flex flex-col gap-1.5">
                <span className="text-xs text-muted">
                  {t('connections.providerDialog.endpoint')}
                  {agentId === 'pi' && !piNeedsUrl ? t('connections.providerDialog.optional') : ''}
                </span>
                <Input
                  value={
                    useOfficial
                      ? official?.displayBaseUrl ||
                        official?.baseUrl ||
                        vars.baseUrl ||
                        t('connections.providerDialog.officialDefault')
                      : vars.baseUrl
                  }
                  onChange={(e) => {
                    if (useOfficial) return;
                    patchVars({ baseUrl: e.target.value });
                  }}
                  onPaste={(e) => {
                    if (useOfficial) return;
                    onFieldPaste('baseUrl', e);
                  }}
                  placeholder={
                    agentId === 'pi' && !piNeedsUrl
                      ? t('connections.providerDialog.officialBuiltinEndpoint')
                      : 'https://api.example.com'
                  }
                  autoComplete="off"
                  spellCheck={false}
                  readOnly={useOfficial}
                  className={useOfficial ? 'cursor-default bg-canvas text-secondary' : undefined}
                />
                {piNeedsUrl && !vars.baseUrl.trim() ? (
                  <span className="text-meta text-danger">{t('connections.providerDialog.customSlotNeedsUrl')}</span>
                ) : null}
              </label>
              <label className="flex flex-col gap-1.5">
                <span className="text-xs text-muted">
                  {t('connections.apiKeyDialog.key')}
                </span>
                <SecretInput
                  value={vars.apiKey}
                  onChange={(v) => patchVars({ apiKey: v })}
                  placeholder={isEdit
                    ? t('connections.apiKeyDialog.keyPlaceholderEdit')
                    : t('connections.apiKeyDialog.keyPlaceholderAdd')}
                />
                {keyHint ? <p className="text-meta text-muted">{keyHint}</p> : null}
              </label>
              <label className="flex flex-col gap-1.5">
                <span className="text-xs text-muted">
                  {t('connections.providerDialog.model')}
                  {agentId === 'pi' && !piNeedsUrl ? t('connections.providerDialog.optional') : ''}
                </span>
                <SuggestableInput
                  value={useOfficial ? official?.model || vars.model : vars.model}
                  onChange={(v) => {
                    if (useOfficial) return;
                    patchVars({ model: v });
                  }}
                  suggestions={!useOfficial && remoteStatus.showPicker ? remoteModels : undefined}
                  placeholder={
                    agentId === 'pi' && !piNeedsUrl
                      ? t('connections.providerDialog.officialBuiltinModel')
                      : t('connections.providerDialog.modelId')
                  }
                  readOnly={useOfficial}
                  className={useOfficial ? 'cursor-default bg-canvas text-secondary' : undefined}
                  statusLabel={modelFieldStatus?.label}
                  statusRetry={modelFieldStatus?.onRetry}
                />
              </label>
            </>
          ) : null}

          <div className="flex flex-col gap-2">
            <button
              type="button"
              className="flex items-center gap-1 self-start text-xs text-muted hover:text-secondary"
              onClick={() => setShowAdvanced((open) => !open)}
            >
              <ChevronDown
                className={cn(
                  'h-3.5 w-3.5 transition-transform',
                  showAdvanced && 'rotate-180',
                )}
              />
              {t('connections.providerDialog.advanced', { format: configFormat.toUpperCase() })}
              {useOfficial ? t('connections.providerDialog.advancedReadonly') : ''}
              {t('connections.providerDialog.advancedClose')}
            </button>
            {showAdvanced ? (
              <ConfigEditor
                value={configText === REDACTED_MARKER ? '' : configText}
                format={configFormat}
                onChange={useOfficial ? () => {} : onConfigTextChange}
                readOnly={useOfficial}
              />
            ) : null}
          </div>
        </div>
  );

  if (asPanel) {
    if (!open) return null;
    return (
      <SideInspectPanel
        title={title}
        onClose={requestClose}
        headerActions={headerActions}
        width={width}
      >
        {form}
      </SideInspectPanel>
    );
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(v) => {
        if (!v) requestClose();
        else onOpenChange(true);
      }}
    >
      <DialogContent className="max-h-[90vh] max-w-lg overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        {form}
        <DialogFooter>
          {headerActions}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
