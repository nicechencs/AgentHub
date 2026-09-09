import { useCallback, useEffect, useState } from 'react';
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
import type { AgentKey } from '@/lib/types';
import { canInstallListedPlugin } from './can-install';

const INSTALL_AGENTS: AgentKey[] = ['grok', 'claude'];

function componentSummary(components: PluginComponent[]): string {
  if (components.length === 0) return '';
  return components.map((item) => item.name).join(' · ');
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
  const [previewing, setPreviewing] = useState(false);
  const [trusted, setTrusted] = useState(false);

  const loadAvailable = useCallback(async (nextAgent: AgentKey) => {
    setLoadingAvailable(true);
    setAvailableError(null);
    try {
      setAvailable(await listAvailablePlugins(nextAgent));
    } catch (e) {
      setAvailable([]);
      setAvailableError(e);
    } finally {
      setLoadingAvailable(false);
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    const next = canInstallListedPlugin(defaultAgent) ? (defaultAgent as AgentKey) : 'grok';
    setAgent(next);
    setSource('');
    setPreview(null);
    setPreviewError(null);
    setTrusted(false);
    void loadAvailable(next);
  }, [open, defaultAgent, loadAvailable]);

  async function selectPack(pack: PluginEntry) {
    const spec =
      agent === 'claude' && pack.marketplace ? `${pack.name}@${pack.marketplace}` : pack.name;
    setSource(spec);
    setPreview(pack);
    setPreviewError(null);
  }

  async function runPreview() {
    const trimmed = source.trim();
    if (!trimmed) return;
    setPreviewing(true);
    setPreviewError(null);
    try {
      setPreview(await previewPluginInstall(agent, trimmed));
    } catch (e) {
      setPreview(null);
      setPreviewError(e);
    } finally {
      setPreviewing(false);
    }
  }

  async function chooseLocalDir() {
    try {
      const dir = await pickDirectory({ title: t('plugins.install.pickDir') });
      if (!dir) return;
      setSource(dir);
      setPreviewing(true);
      setPreviewError(null);
      try {
        setPreview(await previewPluginInstall(agent, dir));
      } catch (e) {
        setPreview(null);
        setPreviewError(e);
      } finally {
        setPreviewing(false);
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
  const canSubmit =
    Boolean(source.trim()) &&
    Boolean(preview) &&
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
                    setTrusted(false);
                    void loadAvailable(id);
                  }}
                >
                  {agentDisplayName(id)}
                </Button>
              ))}
            </div>
          </div>

          <label className="flex flex-col gap-1">
            <span className="text-meta text-muted">{t('plugins.install.source')}</span>
            <Input
              value={source}
              disabled={busy}
              placeholder={
                agent === 'claude'
                  ? t('plugins.install.sourcePlaceholderClaude')
                  : t('plugins.install.sourcePlaceholderGrok')
              }
              onChange={(e) => {
                setSource(e.target.value);
                setPreview(null);
                setPreviewError(null);
              }}
              onBlur={() => {
                if (source.trim()) void runPreview();
              }}
            />
          </label>
          {agent === 'grok' ? (
            <div>
              <Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => void chooseLocalDir()}>
                {t('plugins.install.pickDir')}
              </Button>
            </div>
          ) : null}

          <div>
            <p className="mb-1 text-meta text-muted">{t('plugins.install.available')}</p>
            {loadingAvailable ? (
              <p className="text-meta text-muted">{t('plugins.page.refresh')}</p>
            ) : availableError ? (
              <ErrorState
                compact
                error={availableError}
                title={t('plugins.install.availableFailed')}
                onRetry={() => void loadAvailable(agent)}
              />
            ) : available.length === 0 ? (
              <p className="text-meta text-muted">{t('plugins.install.availableEmpty')}</p>
            ) : (
              <ul className="max-h-40 overflow-y-auto rounded-card border border-border">
                {available.map((pack) => (
                  <li key={pack.id}>
                    <button
                      type="button"
                      className="flex w-full flex-col items-start gap-0.5 px-3 py-2 text-left hover:bg-hover"
                      disabled={busy}
                      onClick={() => void selectPack(pack)}
                    >
                      <span className="text-body font-medium">{pack.name}</span>
                      {pack.description ? (
                        <span className="line-clamp-1 text-meta text-secondary">{pack.description}</span>
                      ) : null}
                    </button>
                  </li>
                ))}
              </ul>
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
                title={t('plugins.install.needPreview')}
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
