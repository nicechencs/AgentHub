/**
 * API Key 设置弹窗（产品层已合并原「供应商」）。
 * - 默认勾选「官方」：带出官方 URL / 模型
 * - 取消勾选：自定义端点与模型
 * - 配置保存：Catalog 声明有 Projector 时 fail-closed（validate/materialize）；
 *   仅 configSchemaVersion === null 时走 legacy applyFormVars。
 */
import * as React from 'react';
import { ChevronDown, FolderOpen, Sparkles } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ConfigEditor } from '@/components/shared/ConfigEditor';
import { GenericConfigForm } from '@/components/shared/GenericConfigForm';
import { SecretInput } from '@/components/shared/SecretInput';
import { useToast } from '@/components/ui/toast';
import { useAgentCatalogOptional } from '@/app/runtime';
import { agentDisplayName } from '@/config/agents';
import {
  looksLikeOfficialEndpoint,
  officialApiDefaults,
} from '@/config/official-api';
import {
  getAgentConfigSchema,
  materializeAgentConfig,
  validateAgentConfig,
  type AgentConfigSchemaDto,
} from '@/lib/api/config';
import { openAgentConfigDir } from '@/lib/api/install';
import { upsertProvider } from '@/lib/api/provider';
import type { AgentId, Provider } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  applyFormVars,
  applySmartPaste,
  defaultConfigScaffold,
  EMPTY_FORM_VARS,
  extractFormVars,
  initFormFromConfig,
  liveConfigPaths,
  parseJsonObjectConfig,
  REDACTED_MARKER,
  smartDetectUrlAndKey,
  type ProviderFormVars,
} from '@/lib/provider-detect';
import {
  canSaveWithSchemaStatus,
  planSchemaLoad,
  resolveProjectorExpectation,
  runProviderSaveFlow,
  schemaErrorMessage,
  type SchemaUiStatus,
} from './providerSaveFlow';

export type ProviderDialogMode = 'add' | 'edit';

export function getConfigTextError(
  agentId: AgentId,
  configText: string,
  configFormat: 'json' | 'toml',
): string | null {
  if (configFormat !== 'json' && agentId !== 'claude') return null;
  const trimmed = configText.trim();
  if (!trimmed || trimmed === REDACTED_MARKER) return null;
  const parsed = parseJsonObjectConfig(configText);
  return parsed.ok ? null : parsed.message;
}

export function ProviderEditDialog({
  agentId,
  open,
  onOpenChange,
  mode = 'add',
  provider,
  onSaved,
}: {
  agentId: AgentId;
  open: boolean;
  onOpenChange: (v: boolean) => void;
  mode?: ProviderDialogMode;
  provider?: Provider | null;
  onSaved: (p: Provider) => void;
}) {
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
  const [showAdvanced, setShowAdvanced] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  /** 默认官方：带出官方 URL / 模型 */
  const [useOfficial, setUseOfficial] = React.useState(true);
  /** Backend config schema when Catalog declares a projector. */
  const [configSchema, setConfigSchema] = React.useState<AgentConfigSchemaDto | null>(
    null,
  );
  const [schemaStatus, setSchemaStatus] = React.useState<SchemaUiStatus>('idle');
  const [schemaError, setSchemaError] = React.useState<string | null>(null);
  /** Bump to re-run schema load without clearing form fields. */
  const [schemaLoadToken, setSchemaLoadToken] = React.useState(0);

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
    setSchemaLoadToken((t) => t + 1);
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
      'reasoningEffort',
      'wireApi',
      'providerSlug',
    ]);
  }, [useOfficial]);

  const applyOfficialDefaults = React.useCallback(
    (keepKey?: string) => {
      const off = officialApiDefaults(agentId);
      if (!off) {
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
      setConfigFormat(off.format);
      setConfigText(off.scaffoldText);
      setConfigError(null);
      const extracted = extractFormVars(agentId, off.scaffoldText, off.format);
      setVars({
        ...extracted,
        baseUrl: off.baseUrl,
        model: off.model,
        modelOpus: off.modelOpus ?? extracted.modelOpus,
        modelSonnet: off.modelSonnet ?? extracted.modelSonnet,
        modelHaiku: off.modelHaiku ?? extracted.modelHaiku,
        modelFable: off.modelFable ?? extracted.modelFable,
        modelSubagent: off.modelSubagent ?? extracted.modelSubagent,
        apiKey: keepKey ?? '',
      });
      // 再写回一遍，确保 model/url 进正文
      setConfigText(
        applyFormVars(agentId, off.scaffoldText, off.format, {
          ...extracted,
          baseUrl: off.baseUrl,
          model: off.model,
          modelOpus: off.modelOpus ?? '',
          modelSonnet: off.modelSonnet ?? '',
          modelHaiku: off.modelHaiku ?? '',
          modelFable: off.modelFable ?? '',
          modelSubagent: off.modelSubagent ?? '',
          apiKey: keepKey ?? '',
        }),
      );
    },
    [agentId],
  );

  React.useEffect(() => {
    if (!open) return;
    setPasteBuf('');
    setDetectHints([]);
    if (isEdit && provider) {
      setName(provider.name ?? '');
      setConfigText(provider.configText);
      setConfigFormat(provider.configFormat);
      setConfigError(getConfigTextError(agentId, provider.configText, provider.configFormat));
      const nextVars = initFormFromConfig(
        agentId,
        provider.configText,
        provider.configFormat,
        provider.authApiKey,
      );
      setVars(nextVars);
      const inferredOfficial =
        provider.official === true ||
        (provider.official !== false &&
          looksLikeOfficialEndpoint(agentId, nextVars.baseUrl));
      setUseOfficial(inferredOfficial);
      setShowAdvanced(provider.configText.trim() === REDACTED_MARKER);
      return;
    }
    // 新增：默认官方
    setName('');
    setConfigError(null);
    setUseOfficial(true);
    setShowAdvanced(false);
    applyOfficialDefaults();
  }, [open, isEdit, provider, agentId, applyOfficialDefaults]);

  const onToggleOfficial = (checked: boolean) => {
    setUseOfficial(checked);
    if (checked) {
      applyOfficialDefaults(vars.apiKey);
      if (!name.trim() && official) {
        setName(official.label);
      }
    } else {
      // 切到自定义：保留 key，换中转占位骨架
      const scaffold = defaultConfigScaffold(agentId);
      setConfigFormat(scaffold.format);
      const extracted = extractFormVars(agentId, scaffold.text, scaffold.format);
      const next = { ...extracted, apiKey: vars.apiKey };
      setVars(next);
      setConfigText(applyFormVars(agentId, scaffold.text, scaffold.format, next));
      setConfigError(null);
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
      setConfigError(getConfigTextError(agentId, result.configText, result.configFormat));
      if (opts?.fillName !== false && result.suggestedName) {
        setName((n) => n.trim() || result.suggestedName || '');
      }
      return result;
    },
    [agentId, configText, configFormat, vars],
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
      setConfigError(getConfigTextError(agentId, nextConfigText, configFormat));
      return next;
    });
  };

  const onSmartPaste = () => {
    const result = runSmartPaste(pasteBuf, { fillName: true });
    if (result.detect.baseUrl || result.detect.apiKey) {
      toast({
        title: '已识别',
        description: result.detect.hints.join(' · '),
        variant: 'success',
      });
    } else {
      toast({
        title: '未识别到 URL / API Key',
        description: '请检查粘贴内容，或直接在下方字段填写',
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
    setConfigError(getConfigTextError(agentId, text, configFormat));
    const extracted = extractFormVars(agentId, text, configFormat);
    const hit = smartDetectUrlAndKey(text);
    setVars({
      ...extracted,
      baseUrl: extracted.baseUrl || hit.baseUrl || '',
      apiKey: extracted.apiKey || hit.apiKey || '',
      model: extracted.model || hit.model || '',
    });
  };

  // 新增必须填 Key；编辑可只改名称/官方开关（Key 留空保留）
  // schema idle/loading/error → fail closed，禁止保存
  const canSave =
    canSaveWithSchemaStatus(schemaStatus) &&
    !configError &&
    (isEdit ? true : Boolean(vars.apiKey.trim()));

  const openLiveDir = async () => {
    try {
      const path = await openAgentConfigDir(agentId);
      toast({
        title: '已打开配置目录',
        description: path,
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: '打开配置目录失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    }
  };

  const save = async () => {
    if (configError) {
      toast({
        title: '配置原文无效',
        description: `${configError}。请修正 JSON 后再保存，当前原文不会被覆盖。`,
        variant: 'danger',
      });
      return;
    }
    if (!canSaveWithSchemaStatus(schemaStatus)) {
      toast({
        title: '暂不可保存',
        description: schemaError ?? '配置 schema 未就绪',
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
      const saveVars: ProviderFormVars =
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
          : vars;
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
          title: isEdit ? '更新失败' : '添加失败',
          description: result.message,
          variant: 'danger',
        });
        // preserveInput: leave form state untouched
        return;
      }

      const endpointLabel = useOfficial ? '官方端点' : '自定义端点';
      toast({
        title: isEdit ? 'API Key 已更新' : 'API Key 已添加',
        description: result.provider.isCurrent
          ? `${result.provider.name} · ${endpointLabel} · 已写入本机配置`
          : `${result.provider.name} · ${endpointLabel} · 已保存到连接池，切换后才会写入本机`,
        variant: 'success',
      });
      onSaved(result.provider);
      onOpenChange(false);
    } catch (e) {
      toast({
        title: isEdit ? '更新失败' : '添加失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] max-w-lg overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {isEdit ? '编辑 API Key' : '添加 API Key'} — {agentName}
          </DialogTitle>
          <DialogDescription>
            勾选「官方」时使用官方服务地址与模型；取消后可填自定义服务地址。当前连接保存后会写入本机配置文件
            <span className="font-mono text-2xs"> {livePaths.config}</span>
            {livePaths.auth ? (
              <>
                {' · '}
                <span className="font-mono text-2xs">{livePaths.auth}</span>
              </>
            ) : null}
            。
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <div className="flex flex-wrap items-start justify-between gap-2 rounded-card border border-border bg-canvas px-3 py-2 text-2xs text-muted">
            <div className="min-w-0 flex-1 space-y-0.5">
              <p>
                <span className="text-secondary">本机当前配置 </span>
                <code className="break-all font-mono">{livePaths.config}</code>
              </p>
              {livePaths.auth && (
                <p>
                  <span className="text-secondary">本机登录凭据 </span>
                  <code className="break-all font-mono">{livePaths.auth}</code>
                </p>
              )}
              <p className="text-muted">{livePaths.hint}</p>
            </div>
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="shrink-0"
              onClick={() => void openLiveDir()}
              title={`打开本机配置目录：${livePaths.openDir}`}
            >
              <FolderOpen className="h-3.5 w-3.5" /> 打开目录
            </Button>
          </div>

          <label className="flex cursor-pointer items-start gap-2.5 rounded-card border border-border bg-panel px-3 py-2.5">
            <input
              type="checkbox"
              className="mt-0.5 h-4 w-4 accent-[var(--color-accent,theme(colors.sky.500))]"
              checked={useOfficial}
              onChange={(e) => onToggleOfficial(e.target.checked)}
            />
            <span className="min-w-0 flex-1">
              <span className="block text-sm font-medium text-primary">使用官方端点</span>
              <span className="mt-0.5 block text-2xs text-muted">
                {official
                  ? `勾选后下方 URL / 模型显示 ${official.label} 默认值且不可改；取消后可自定义中转`
                  : '该 Agent 暂无内置官方模板，可手动填写'}
              </span>
            </span>
          </label>

          {/* 布局固定：勾选官方只切换只读/禁用，不卸载区块，避免高度跳动 */}
          <div
            className={cn(
              'flex flex-col gap-1.5 rounded-card border border-border bg-canvas p-3 transition-opacity',
              useOfficial && 'pointer-events-none opacity-45',
            )}
            aria-disabled={useOfficial || undefined}
          >
            <span className="flex items-center gap-1 text-xs font-medium text-secondary">
              <Sparkles className="h-3.5 w-3.5" />
              智能识别
              {useOfficial ? (
                <span className="font-normal text-muted">（官方模式下不可用）</span>
              ) : (
                <span className="font-normal text-muted">（自定义中转）</span>
              )}
            </span>
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
              placeholder={
                '粘贴中转配置…\n例如 JSON / TOML / BASE_URL=… / sk-… / https://…'
              }
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
                识别并填入
              </Button>
              <span className="text-2xs text-muted">
                {detectHints.length > 0 ? detectHints.join(' · ') : '\u00a0'}
              </span>
            </div>
          </div>

          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">名称</span>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={
                useOfficial && official ? official.label : '可空，默认用域名或「API Key」'
              }
              autoComplete="off"
            />
          </label>

          {/* 固定占位，避免 tomlOpaque 出现时把表单顶高 */}
          <p
            className={cn(
              'min-h-[2.25rem] rounded-btn border px-2.5 py-2 text-2xs',
              tomlOpaque
                ? 'border-border bg-canvas text-muted'
                : 'border-transparent text-transparent',
            )}
            aria-hidden={!tomlOpaque}
          >
            {tomlOpaque
              ? '当前 TOML 在列表中整段脱敏。填写下方 URL/Key 将基于模板重建。'
              : '\u00a0'}
          </p>

          {configError ? (
            <p
              role="alert"
              className="rounded-btn border border-danger/40 bg-danger/5 px-2.5 py-2 text-2xs text-danger"
            >
              {configError}。请修正 JSON 原文后再保存；当前原文会保留，不会被结构化字段覆盖。
            </p>
          ) : null}

          {schemaStatus === 'loading' || schemaStatus === 'idle' ? (
            <p className="text-2xs text-muted">加载配置字段 schema…</p>
          ) : null}

          {schemaStatus === 'error' ? (
            <div className="flex flex-col gap-2 rounded-card border border-danger/40 bg-danger/5 px-3 py-2.5">
              <p className="text-2xs text-danger">
                {schemaError ?? '配置 schema 不可用，禁止保存'}
              </p>
              <Button
                type="button"
                size="sm"
                variant="outline"
                className="self-start"
                onClick={retrySchemaLoad}
              >
                重试
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
            />
          ) : schemaStatus === 'unsupported' ? (
            <>
              <label className="flex flex-col gap-1.5">
                <span className="text-xs text-muted">Endpoint URL</span>
                <Input
                  value={
                    useOfficial
                      ? official?.displayBaseUrl ||
                        official?.baseUrl ||
                        vars.baseUrl ||
                        '（官方默认）'
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
                  placeholder="https://api.example.com"
                  autoComplete="off"
                  spellCheck={false}
                  readOnly={useOfficial}
                  className={useOfficial ? 'cursor-default bg-canvas text-secondary' : undefined}
                />
              </label>
              <label className="flex flex-col gap-1.5">
                <span className="text-xs text-muted">
                  API Key{isEdit ? '（留空保留原密钥）' : ''}
                </span>
                <SecretInput
                  value={vars.apiKey}
                  onChange={(v) => patchVars({ apiKey: v })}
                  placeholder={isEdit ? '输入新密钥以替换…' : 'sk-… / cr_…'}
                />
              </label>
              <label className="flex flex-col gap-1.5">
                <span className="text-xs text-muted">Model</span>
                <Input
                  value={useOfficial ? official?.model || vars.model : vars.model}
                  onChange={(e) => {
                    if (useOfficial) return;
                    patchVars({ model: e.target.value });
                  }}
                  placeholder="模型 id"
                  autoComplete="off"
                  spellCheck={false}
                  readOnly={useOfficial}
                  className={useOfficial ? 'cursor-default bg-canvas text-secondary' : undefined}
                />
              </label>
            </>
          ) : null}

          <div className="flex flex-col gap-2">
            <button
              type="button"
              className="flex items-center gap-1 self-start text-xs text-muted hover:text-secondary"
              onClick={() => setShowAdvanced((v) => !v)}
            >
              <ChevronDown
                className={cn(
                  'h-3.5 w-3.5 transition-transform',
                  showAdvanced && 'rotate-180',
                )}
              />
              高级配置（{configFormat.toUpperCase()} 原文
              {useOfficial ? ' · 只读' : ''}）
            </button>
            {/* 用 CSS 隐藏而非卸载，避免展开态切换官方时塌缩；折叠时仍按 showAdvanced */}
            {showAdvanced && (
              <ConfigEditor
                value={configText === REDACTED_MARKER ? '' : configText}
                format={configFormat}
                onChange={useOfficial ? () => {} : onConfigTextChange}
                readOnly={useOfficial}
              />
            )}
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button disabled={!canSave || saving} onClick={() => void save()}>
            {saving ? '保存中…' : isEdit ? '保存修改' : '添加'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
