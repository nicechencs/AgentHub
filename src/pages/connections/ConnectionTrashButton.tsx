import * as React from 'react';
import { Loader2, RotateCcw, Trash2 } from 'lucide-react';
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
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import type { ConnectionTrashItem } from '@/lib/backend/contracts';
import {
  listConnectionTrash,
  permanentlyDeleteConnectionTrash,
  restoreConnectionTrash,
} from '@/lib/api/trash';
import { agentDisplayName } from '@/config/agents';
import type { AgentId } from '@/lib/types';
import {
  claimConnectionTrashBusy,
  getConnectionTrashBusyIds,
  releaseConnectionTrashBusy,
  subscribeConnectionTrashBusy,
} from './connection-trash-lock';
import { dedupTrashItems, humanizeTrashLabel } from './connection-trash-model';

function dateLabel(value: string): string {
  const isoLike = value.replace(' ', 'T');
  const normalized = /(?:Z|[+-]\d{2}:?\d{2})$/i.test(isoLike) ? isoLike : `${isoLike}Z`;
  const date = new Date(normalized);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export function ConnectionTrashButton({
  agentId,
  onChanged,
}: {
  agentId?: AgentId;
  onChanged?: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [open, setOpen] = React.useState(false);
  const [items, setItems] = React.useState<ConnectionTrashItem[]>([]);
  const [loading, setLoading] = React.useState(false);
  const busyIds = React.useSyncExternalStore(
    subscribeConnectionTrashBusy,
    getConnectionTrashBusyIds,
    getConnectionTrashBusyIds,
  );
  const [pendingPermanent, setPendingPermanent] = React.useState<ConnectionTrashItem | null>(null);

  const trashBusy = busyIds.size > 0;
  const permanentBusy = pendingPermanent !== null && busyIds.has(pendingPermanent.id);
  const trashLocked = trashBusy || pendingPermanent !== null;

  const load = React.useCallback(async () => {
    setLoading(true);
    try {
      setItems(dedupTrashItems(await listConnectionTrash(agentId)));
    } catch (error) {
      toast({
        title: t('connections.trash.loadFailed'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setLoading(false);
    }
  }, [agentId, toast, t]);

  const restore = async (item: ConnectionTrashItem) => {
    if (!claimConnectionTrashBusy(item.id)) return;
    try {
      await restoreConnectionTrash(item.id);
      setItems((current) => current.filter((row) => row.id !== item.id));
      toast({
        title: t('connections.trash.restored'),
        description: t('connections.trash.restoredDesc'),
      });
      onChanged?.();
    } catch (error) {
      toast({
        title: t('connections.trash.restoreFailed'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      releaseConnectionTrashBusy(item.id);
    }
  };

  const confirmPermanentDelete = async () => {
    if (!pendingPermanent) return;
    const item = pendingPermanent;
    if (!claimConnectionTrashBusy(item.id)) return;
    try {
      await permanentlyDeleteConnectionTrash(item.id);
      setItems((current) => current.filter((row) => row.id !== item.id));
      setPendingPermanent(null);
      toast({ title: t('connections.trash.permanentlyDeleted') });
    } catch (error) {
      toast({
        title: t('connections.trash.permanentFailed'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      releaseConnectionTrashBusy(item.id);
    }
  };

  return (
    <>
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="gap-2"
        onClick={() => {
          setOpen(true);
          void load();
        }}
      >
        <Trash2 className="h-4 w-4" aria-hidden />
        {t('connections.trash.button')}
      </Button>

      <Dialog
        open={open}
        onOpenChange={(next) =>
          closeConfirmationOnOpenChange(next, trashLocked, () => {
            setOpen(false);
            setPendingPermanent(null);
          })
        }
      >
        <DialogContent
          className="max-w-xl"
          hideClose={trashLocked}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(trashLocked, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(trashLocked, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(trashLocked, event)}
        >
          <DialogHeader>
            <DialogTitle>{t('connections.trash.title')}</DialogTitle>
            <DialogDescription>
              {t('connections.trash.description')}
            </DialogDescription>
          </DialogHeader>

          <div className="max-h-[55vh] space-y-2 overflow-y-auto py-2">
            {loading ? (
              <div className="flex items-center justify-center gap-2 py-8 text-body text-muted">
                <Loader2 className="h-4 w-4 animate-spin" />
                {t('connections.trash.loading')}
              </div>
            ) : items.length === 0 ? (
              <p className="py-8 text-center text-body text-muted">{t('connections.trash.empty')}</p>
            ) : (
              items.map((item) => {
                const agentName = agentDisplayName(item.agentId);
                const kindLabel =
                  item.kind === 'account' && item.account?.kind === 'oauth'
                    ? t('kind.oauth')
                    : t('kind.apikey');
                const title = humanizeTrashLabel(item, t);
                return (
                  <div
                    key={item.id}
                    className="flex items-center justify-between gap-3 rounded-card border border-border p-3"
                  >
                    <div className="min-w-0">
                      <p className="truncate text-body font-medium">{title}</p>
                      <p className="text-meta text-muted">
                        {t('connections.trash.deletedAt', {
                          agent: agentName,
                          kind: kindLabel,
                          when: dateLabel(item.deletedAt),
                        })}
                      </p>
                      <p className="text-meta text-muted">
                        {item.wasCurrent ? t('connections.trash.wasCurrent') : ''}
                        {t('connections.trash.expiresAt', { when: dateLabel(item.expiresAt) })}
                      </p>
                    </div>
                    <div className="flex shrink-0 gap-2">
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={trashBusy}
                        onClick={() => void restore(item)}
                      >
                        <RotateCcw className="mr-1 h-3.5 w-3.5" />
                        {t('common.restore')}
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="text-destructive"
                        disabled={trashBusy}
                        onClick={() => setPendingPermanent(item)}
                      >
                        {t('connections.trash.permanent')}
                      </Button>
                    </div>
                  </div>
                );
              })
            )}
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => void load()} disabled={loading || trashLocked}>
              {t('connections.trash.refresh')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={!!pendingPermanent}
        onOpenChange={(next) =>
          closeConfirmationOnOpenChange(next, permanentBusy, () => setPendingPermanent(null))
        }
      >
        <DialogContent
          className="max-w-sm"
          hideClose={permanentBusy}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(permanentBusy, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(permanentBusy, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(permanentBusy, event)}
        >
          <DialogHeader>
            <DialogTitle>
              {t('connections.trash.permanentTitle', {
                label: pendingPermanent ? humanizeTrashLabel(pendingPermanent, t) : '',
              })}
            </DialogTitle>
            <DialogDescription>{t('connections.trash.permanentDesc')}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              type="button"
              variant="secondary"
              disabled={permanentBusy}
              onClick={() => setPendingPermanent(null)}
            >
              {t('common.cancel')}
            </Button>
            <Button
              type="button"
              variant="danger"
              disabled={permanentBusy}
              onClick={() => void confirmPermanentDelete()}
            >
              {permanentBusy ? t('connections.trash.deleting') : t('common.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
