import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { PluginEntry } from '@/lib/backend/contracts/plugin-types';

export type PluginUpdateTarget =
  | { kind: 'plugin'; plugin: PluginEntry }
  | { kind: 'pi-all' };

export function PluginUpdateDialog({
  target,
  busy,
  error,
  onClose,
  onConfirm,
}: {
  target: PluginUpdateTarget | null;
  busy: boolean;
  error: unknown;
  onClose: () => void;
  onConfirm: (target: PluginUpdateTarget) => Promise<void>;
}) {
  const { t } = useI18n();
  const piAll = target?.kind === 'pi-all';

  return (
    <Dialog
      open={target !== null}
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {piAll ? t('plugins.update.piTitle') : t('plugins.update.title')}
          </DialogTitle>
          <DialogDescription>
            {piAll ? t('plugins.update.piDescription') : t('plugins.update.description')}
          </DialogDescription>
        </DialogHeader>
        {target?.kind === 'plugin' ? (
          <p className="text-sm font-medium">{target.plugin.name}</p>
        ) : null}
        {error ? (
          <ErrorState
            title={t('plugins.update.failed')}
            error={error}
            onRetry={() => {
              if (target) void onConfirm(target);
            }}
          />
        ) : null}
        <DialogFooter>
          <Button variant="secondary" disabled={busy} onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button disabled={busy || !target} onClick={() => target && void onConfirm(target)}>
            {busy ? t('plugins.update.updating') : t('plugins.update.confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
