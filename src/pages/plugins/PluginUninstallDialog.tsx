import { useEffect, useState } from 'react';
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
import { agentDisplayName } from '@/config/agents';
import type { PluginEntry } from '@/lib/backend/contracts/plugin-types';

export function PluginUninstallDialog({
  plugin,
  busy,
  error,
  onClose,
  onUninstall,
}: {
  plugin: PluginEntry | null;
  busy: boolean;
  error: unknown;
  onClose: () => void;
  onUninstall: (plugin: PluginEntry, keepData: boolean) => Promise<void>;
}) {
  const { t } = useI18n();
  const [deleteData, setDeleteData] = useState(false);

  useEffect(() => {
    if (plugin) setDeleteData(false);
  }, [plugin]);

  const open = plugin !== null;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => closeConfirmationOnOpenChange(next, busy, onClose)}
    >
      <DialogContent
        hideClose={busy}
        onPointerDownOutside={(event) => preventBusyConfirmationDismissal(busy, event)}
        onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(busy, event)}
      >
        <DialogHeader>
          <DialogTitle>{t('plugins.uninstall.title')}</DialogTitle>
          <DialogDescription>{t('plugins.uninstall.description')}</DialogDescription>
        </DialogHeader>
        {plugin ? (
          <div className="flex flex-col gap-3">
            <p className="text-body">
              {agentDisplayName(plugin.agent)} · {plugin.name}
            </p>
            {plugin.components.length > 0 ? (
              <p className="text-meta text-secondary">
                {plugin.components.map((item) => item.name).join(' · ')}
              </p>
            ) : null}
            <label className="flex items-start gap-2 text-body">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={deleteData}
                disabled={busy}
                onChange={(e) => setDeleteData(e.target.checked)}
              />
              <span>
                <span className="block font-medium">{t('plugins.uninstall.deleteData')}</span>
                <span className="block text-meta text-muted">
                  {t('plugins.uninstall.deleteDataHint')}
                </span>
              </span>
            </label>
            {error ? (
              <ErrorState
                compact
                error={error}
                title={t('plugins.uninstall.failed')}
                onRetry={() => void onUninstall(plugin, !deleteData)}
              />
            ) : null}
          </div>
        ) : null}
        <DialogFooter>
          <Button type="button" variant="secondary" disabled={busy} onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button
            type="button"
            variant="danger"
            disabled={busy || !plugin}
            onClick={() => {
              if (plugin) void onUninstall(plugin, !deleteData);
            }}
          >
            {busy ? t('plugins.uninstall.uninstalling') : t('plugins.uninstall.confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
