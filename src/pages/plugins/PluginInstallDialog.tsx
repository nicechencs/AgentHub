import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { pickDirectory } from '@/lib/api/settings';
import {
  listAvailablePlugins,
  previewPluginInstall,
} from '@/lib/api/plugins';
import { agentDisplayName } from '@/config/agents';
import type { PluginComponent, PluginEntry } from '@/lib/backend/contracts/plugin-types';
import type { TranslateFn } from '@/lib/i18n';
import type { AgentKey } from '@/lib/types';
import { canInstallListedPlugin } from './can-install';
import { isPiAbsoluteLocalSource } from './plugin-install-source';

const INSTALL_AGENTS: AgentKey[] = ['claude', 'codex', 'grok', 'pi'];
type PiSourceKind = 'npm' | 'git' | 'local';

function componentSummary(components: PluginComponent[]): string {
  if (components.length === 0) return '';
  return components.map((item) => item.name).join(' · ');
}

function piSourceKindLabel(kind: PiSourceKind, t: TranslateFn): string {
  switch (kind) {
    case 'npm':
      return t('plugins.install.sourceKindNpm');
    case 'git':
      return t('plugins.install.sourceKindGit');
    case 'local':
      return t('plugins.install.sourceKindLocal');
  }
}

function piSourcePlaceholder(kind: PiSourceKind, t: TranslateFn): string {
  switch (kind) {
    case 'npm':
      return t('plugins.install.sourcePlaceholderPiNpm');
    case 'git':
      return t('plugins.install.sourcePlaceholderPiGit');
    case 'local':
      return t('plugins.install.sourcePlaceholderPiLocal');
  }
}

export function PluginInstallDialog({
  open,
  defaultAgent,
  busy,
  error,
  onClose,
  onInstall,
}: {
  open: boolean;
  defaultAgent: AgentKey | 'all';
  busy: boolean;
  error: unknown;
  onClose: () => void;
  onInstall: (agent: AgentKey, source: string, confirmed: boolean) => Promise<void>;
}) {
  const { t } = useI18n();
  const initialAgent = canInstallListedPlugin(defaultAgent) ? (defaultAgent as AgentKey) : 'grok';
  const [agent, setAgent] = useState<AgentKey>(initialAgent);
  const [source, setSource] = useState('');
  const [available, setAvailable] = useState<PluginEntry[]>([]);
  const [availableError, setAvailableError] = useState<unknown>(null);
  const [loadingAvailable, setLoadingAvailable] = useState(false);
  const [preview, setPreview] = useState<PluginEntry | null>(null);
  const [previewError, setPreviewError] = useState<unknown>(null);
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [trusted, setTrusted] = useState(false);
  const [codexMarketplace, setCodexMarketplace] = useState('');
  const [piSourceKind, setPiSourceKind] = useState<PiSourceKind>('npm');
  const availableRequest = useRef(0);
  const previewRequest = useRef(0);

  const loadAvailable = useCallback(async (nextAgent: AgentKey) => {
    const request = ++availableRequest.current;
    setLoadingAvailable(true);
    setAvailableError(null);
    if (nextAgent === 'pi') {
      setAvailable([]);
      setLoadingAvailable(false);
      return;
    }
    try {
      const rows = await listAvailablePlugins(nextAgent);
      if (request !== availableRequest.current) return;
      setAvailable(rows);
      if (nextAgent === 'codex') {
        setCodexMarketplace((current) => {
          const marketplaces = rows
            .map((row) => row.marketplace?.trim() ?? '')
            .filter(Boolean);
          return marketplaces.includes(current) ? current : (marketplaces[0] ?? '');
        });
      }
    } catch (e) {
      if (request !== availableRequest.current) return;
      setAvailable([]);
      setAvailableError(e);
    } finally {
      if (request === availableRequest.current) setLoadingAvailable(false);
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    const next = canInstallListedPlugin(defaultAgent) ? (defaultAgent as AgentKey) : 'grok';
    setAgent(next);
    setSource('');
    setPreview(null);
    setPreviewError(null);
    setSourceError(null);
    previewRequest.current += 1;
    setPreviewing(false);
    setTrusted(false);
    setCodexMarketplace('');
    setPiSourceKind('npm');
    void loadAvailable(next);
  }, [open, defaultAgent, loadAvailable]);

  async function selectPack(pack: PluginEntry) {
    const spec =
      pack.installSource ??
      ((agent === 'claude' || agent === 'codex') && pack.marketplace
        ? `${pack.name}@${pack.marketplace}`
        : pack.name);
    setSource(spec);
    previewRequest.current += 1;
    setPreviewing(false);
    setPreview(pack);
    setPreviewError(null);
    setSourceError(null);
  }

  async function runPreview() {
    const trimmed = source.trim();
    if (!trimmed) return;
    if (agent === 'pi' && piSourceKind === 'local' && !isPiAbsoluteLocalSource(trimmed)) {
      setPreview(null);
      setPreviewError(null);
      setSourceError(t('plugins.install.piLocalAbsoluteError'));
      return;
    }
    const requestedAgent = agent;
    const request = ++previewRequest.current;
    setPreviewing(true);
    setSourceError(null);
    setPreviewError(null);
    try {
      const result = await previewPluginInstall(requestedAgent, trimmed);
      if (request !== previewRequest.current) return;
      setPreview(result);
    } catch (e) {
      if (request !== previewRequest.current) return;
      setPreview(null);
      setPreviewError(e);
    } finally {
      if (request === previewRequest.current) setPreviewing(false);
    }
  }

  async function chooseLocalDir() {
    const requestedAgent = agent;
    const request = ++previewRequest.current;
    setPreviewing(false);
    setSourceError(null);
    try {
      const dir = await pickDirectory({ title: t('plugins.install.pickDir') });
      if (!dir || request !== previewRequest.current) return;
      setSource(dir);
      if (requestedAgent === 'pi' && !isPiAbsoluteLocalSource(dir)) {
        setPreview(null);
        setPreviewError(null);
        setSourceError(t('plugins.install.piLocalAbsoluteError'));
        return;
      }
      setPreviewing(true);
      setSourceError(null);
      setPreviewError(null);
      try {
        const result = await previewPluginInstall(requestedAgent, dir);
        if (request !== previewRequest.current) return;
        setPreview(result);
      } catch (e) {
        if (request !== previewRequest.current) return;
        setPreview(null);
        setPreviewError(e);
      } finally {
        if (request === previewRequest.current) setPreviewing(false);
      }
    } catch (e) {
      setPreviewError(e);
    }
  }

  async function confirmInstall() {
    const trimmed = source.trim();
    if (!trimmed) return;
    if (!preview && !previewError) {
      await runPreview();
      return;
    }
    if (agent === 'grok' && !trusted) return;
    await onInstall(agent, trimmed, true);
  }

  const grokNeedsTrust = agent === 'grok';
  const codexMarketplaces = useMemo(
    () => [
      ...new Set(
        available
          .filter((row) => row.agent === 'codex')
          .map((row) => row.marketplace?.trim() ?? '')
          .filter(Boolean),
      ),
    ],
    [available],
  );
  const visibleAvailable =
    agent === 'codex' && codexMarketplace
      ? available.filter((row) => row.marketplace === codexMarketplace)
      : available;
  const sourcePlaceholder =
    agent === 'claude'
      ? t('plugins.install.sourcePlaceholderClaude')
      : agent === 'codex'
        ? t('plugins.install.sourcePlaceholderCodex')
        : agent === 'pi'
          ? piSourcePlaceholder(piSourceKind, t)
          : t('plugins.install.sourcePlaceholderGrok');
  const canSubmit =
    Boolean(source.trim()) &&
    Boolean(preview) &&
    !sourceError &&
    (!grokNeedsTrust || trusted) &&
    !busy &&
    !previewing;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => closeConfirmationOnOpenChange(next, busy, onClose)}
    >
      <DialogContent
        className="max-w-xl"
        hideClose={busy}
        onPointerDownOutside={(event) => preventBusyConfirmationDismissal(busy, event)}
        onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(busy, event)}
      >
        <DialogHeader>
          <DialogTitle>{t('plugins.install.title')}</DialogTitle>
          <DialogDescription>{t('plugins.install.description')}</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <div>
            <p className="mb-1 text-meta text-muted">{t('plugins.install.agent')}</p>
            <div className="flex gap-2">
              {INSTALL_AGENTS.map((id) => (
                <Button
                  key={id}
                  type="button"
                  size="sm"
                  variant={agent === id ? 'default' : 'outline'}
                  disabled={busy}
                  onClick={() => {
                    setAgent(id);
                    setSource('');
                    setPreview(null);
                    setPreviewError(null);
                    setSourceError(null);
                    previewRequest.current += 1;
                    setPreviewing(false);
                    setTrusted(false);
                    setCodexMarketplace('');
                    setPiSourceKind('npm');
                    void loadAvailable(id);
                  }}
                >
                  {agentDisplayName(id)}
                </Button>
              ))}
            </div>
          </div>

          {agent === 'pi' ? (
            <div>
              <p className="mb-1 text-meta text-muted">{t('plugins.install.sourceKind')}</p>
              <div className="flex gap-2">
                {(['npm', 'git', 'local'] as const).map((kind) => (
                  <Button
                    key={kind}
                    type="button"
                    size="sm"
                    variant={piSourceKind === kind ? 'default' : 'outline'}
                    disabled={busy}
                    onClick={() => {
                      setPiSourceKind(kind);
                      setSource('');
                      setPreview(null);
                      setPreviewError(null);
                      setSourceError(null);
                      previewRequest.current += 1;
                      setPreviewing(false);
                    }}
                  >
                    {piSourceKindLabel(kind, t)}
                  </Button>
                ))}
              </div>
            </div>
          ) : null}

          <label className="flex flex-col gap-1">
            <span className="text-meta text-muted">{t('plugins.install.source')}</span>
            <Input
              value={source}
              disabled={busy}
              placeholder={sourcePlaceholder}
              aria-invalid={Boolean(sourceError)}
              onChange={(e) => {
                setSource(e.target.value);
                setPreview(null);
                setPreviewError(null);
                setSourceError(null);
                previewRequest.current += 1;
                setPreviewing(false);
              }}
              onBlur={() => {
                if (source.trim()) void runPreview();
              }}
            />
            {agent === 'pi' && piSourceKind === 'local' ? (
              <span className={sourceError ? 'text-meta text-danger' : 'text-meta text-muted'}>
                {sourceError ?? t('plugins.install.piLocalAbsoluteHint')}
              </span>
            ) : null}
          </label>
          {agent === 'grok' || (agent === 'pi' && piSourceKind === 'local') ? (
            <div>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={busy}
                onClick={() => void chooseLocalDir()}
              >
                {t('plugins.install.pickDir')}
              </Button>
            </div>
          ) : null}

          <div>
            <p className="mb-1 text-meta text-muted">
              {agent === 'pi'
                ? t('plugins.install.availablePi')
                : t('plugins.install.available')}
            </p>
            {loadingAvailable ? (
              <p className="text-meta text-muted">{t('plugins.page.refresh')}</p>
            ) : availableError ? (
              <ErrorState
                compact
                error={availableError}
                title={t('plugins.install.availableFailed')}
                onRetry={() => void loadAvailable(agent)}
              />
            ) : visibleAvailable.length === 0 ? (
              <p className="text-meta text-muted">
                {agent === 'pi'
                  ? t('plugins.install.availableEmptyPi')
                  : t('plugins.install.availableEmpty')}
              </p>
            ) : (
              <>
                {agent === 'codex' && codexMarketplaces.length > 0 ? (
                  <label className="mb-2 flex flex-col gap-1">
                    <span className="text-meta text-muted">
                      {t('plugins.install.marketplace')}
                    </span>
                    <select
                      className="h-9 rounded-btn border border-border bg-background px-2 text-sm"
                      value={codexMarketplace}
                      disabled={busy}
                      onChange={(event) => {
                        setCodexMarketplace(event.target.value);
                        setSource('');
                        setPreview(null);
                        setPreviewError(null);
                        setSourceError(null);
                        previewRequest.current += 1;
                        setPreviewing(false);
                      }}
                    >
                      {codexMarketplaces.map((marketplace) => (
                        <option key={marketplace} value={marketplace}>
                          {marketplace}
                        </option>
                      ))}
                    </select>
                  </label>
                ) : null}
                <ul className="max-h-40 overflow-y-auto rounded-card border border-border">
                  {visibleAvailable.map((pack) => (
                    <li key={pack.id}>
                      <button
                        type="button"
                        className="flex w-full flex-col items-start gap-0.5 px-3 py-2 text-left hover:bg-hover"
                        disabled={busy}
                        onClick={() => void selectPack(pack)}
                      >
                        <span className="text-body font-medium">{pack.name}</span>
                        {agent === 'codex' && pack.marketplace ? (
                          <span className="text-meta text-muted">{pack.marketplace}</span>
                        ) : null}
                        {pack.description ? (
                          <span className="line-clamp-1 text-meta text-secondary">
                            {pack.description}
                          </span>
                        ) : null}
                      </button>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>

          <div>
            <p className="mb-1 text-meta text-muted">{t('plugins.install.preview')}</p>
            {previewing ? (
              <p className="text-meta text-muted">{t('plugins.install.previewHint')}</p>
            ) : previewError ? (
              <ErrorState
                compact
                error={previewError}
                title={t('plugins.install.previewFailed')}
                onRetry={() => void runPreview()}
              />
            ) : preview ? (
              preview.components.length === 0 ? (
                <p className="text-meta text-muted">{t('plugins.install.previewEmpty')}</p>
              ) : (
                <p className="text-meta text-secondary">{componentSummary(preview.components)}</p>
              )
            ) : (
              <p className="text-meta text-muted">{t('plugins.install.previewHint')}</p>
            )}
          </div>

          {grokNeedsTrust ? (
            <label className="flex items-start gap-2 text-body">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={trusted}
                disabled={busy}
                onChange={(e) => setTrusted(e.target.checked)}
              />
              <span>
                <span className="block font-medium">{t('plugins.install.trust')}</span>
                <span className="block text-meta text-muted">{t('plugins.install.trustHint')}</span>
              </span>
            </label>
          ) : null}

          {error ? (
            <ErrorState
              compact
              error={error}
              title={t('plugins.install.failed')}
              onRetry={() => void confirmInstall()}
            />
          ) : null}
        </div>

        <DialogFooter>
          <Button type="button" variant="secondary" disabled={busy} onClick={onClose}>
            {t('common.cancel')}
          </Button>
          {error ? null : (
            <Button type="button" disabled={!canSubmit} onClick={() => void confirmInstall()}>
              {busy ? t('plugins.install.installing') : t('plugins.install.confirm')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
