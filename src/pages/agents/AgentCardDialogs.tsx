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
import { Input } from '@/components/ui/input';

export type AgentCardConfirmKind = 'program' | 'config' | 'force-upgrade' | 'install' | 'oneclick' | null;

export type AgentCardDialogsProps = {
  agentName: string;
  confirmDialog: AgentCardConfirmKind;
  confirmName: string;
  onConfirmNameChange: (v: string) => void;
  uninstalling: boolean;
  busy: boolean;
  updateState: string | undefined;
  onClose: () => void;
  onUninstall: (deleteConfig: boolean) => void;
  onConfirmForceUpgrade: () => void;
  onConfirmInstall: () => void;
  onConfirmOneClick: () => void;
  /** Live read: leftover menu dismiss while the opening click is settling. */
  shouldIgnoreDismiss?: (nextOpen: boolean) => boolean;
};

export function AgentCardDialogs({
  agentName,
  confirmDialog,
  confirmName,
  onConfirmNameChange,
  uninstalling,
  busy,
  updateState,
  onClose,
  onUninstall,
  onConfirmForceUpgrade,
  onConfirmInstall,
  onConfirmOneClick,
  shouldIgnoreDismiss,
}: AgentCardDialogsProps) {
  const { t } = useI18n();
  const onDialogOpenChange = (nextOpen: boolean) => {
    if (shouldIgnoreDismiss?.(nextOpen)) return;
    if (!nextOpen) onClose();
  };

  return (
    <>
      <Dialog open={confirmDialog === 'program'} onOpenChange={onDialogOpenChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('agents.dialog.uninstallTitle', { name: agentName })}</DialogTitle>
            <DialogDescription>
              {t('agents.dialog.uninstallDesc')}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => onClose()} disabled={uninstalling}>
              {t('common.cancel')}
            </Button>
            <Button variant="danger" onClick={() => onUninstall(false)} disabled={uninstalling}>
              {uninstalling ? t('agents.dialog.uninstalling') : t('agents.dialog.confirmUninstall')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={confirmDialog === 'force-upgrade'}
        onOpenChange={onDialogOpenChange}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('agents.dialog.forceUpgradeTitle', { name: agentName })}</DialogTitle>
            <DialogDescription>
              {updateState === 'up_to_date'
                ? t('agents.dialog.forceUpgradeUpToDate')
                : t('agents.dialog.forceUpgradeUnknown')}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => onClose()} disabled={busy}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="default"
              disabled={busy}
              onClick={() => {
                onClose();
                onConfirmForceUpgrade();
              }}
            >
              {t('agents.dialog.confirmForceUpgrade')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={confirmDialog === 'config'}
        onOpenChange={(nextOpen) => {
          if (shouldIgnoreDismiss?.(nextOpen)) return;
          if (!nextOpen) {
            onClose();
            onConfirmNameChange('');
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('agents.dialog.uninstallConfigTitle')}</DialogTitle>
            <DialogDescription className="text-danger">
              {t('agents.dialog.uninstallConfigDesc')}
            </DialogDescription>
          </DialogHeader>
          <div className="text-sm text-secondary">
            {t('agents.dialog.typeToConfirm', { name: agentName })}
          </div>
          <Input
            value={confirmName}
            onChange={(e) => onConfirmNameChange(e.target.value)}
            placeholder={agentName}
            className="mt-2"
            disabled={uninstalling}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => onClose()} disabled={uninstalling}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              onClick={() => onUninstall(true)}
              disabled={uninstalling || confirmName !== agentName}
            >
              {uninstalling ? t('agents.dialog.uninstalling') : t('agents.dialog.uninstallConfigAction')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={confirmDialog === 'install'} onOpenChange={onDialogOpenChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('agents.dialog.installTitle', { name: agentName })}</DialogTitle>
            <DialogDescription>
              {t('agents.dialog.installDesc')}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => onClose()} disabled={busy}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="default"
              disabled={busy}
              onClick={() => {
                onClose();
                onConfirmInstall();
              }}
            >
              {t('agents.dialog.confirmInstall')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={confirmDialog === 'oneclick'} onOpenChange={onDialogOpenChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('agents.dialog.oneClickTitle', { name: agentName })}</DialogTitle>
            <DialogDescription>
              {t('agents.dialog.oneClickDesc')}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => onClose()} disabled={busy}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="default"
              disabled={busy}
              onClick={() => {
                onClose();
                onConfirmOneClick();
              }}
            >
              {t('agents.dialog.confirmOneClick')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
