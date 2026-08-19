import { useEffect, useState } from 'react';
import { FolderOpen } from 'lucide-react';
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
import { Switch } from '@/components/ui/switch';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { Tip } from '@/components/ui/tooltip';
import { pickDirectory } from '@/lib/api/settings';
import type { Conversation } from '@/lib/types';
import {
  autoApproveActive,
  autoApproveConfirmCopy,
  autoApproveEffect,
  autoApproveHint,
} from './chat-model';

export function ChatSettingsDialog({
  open,
  onOpenChange,
  active,
  dangerConfirm,
  onDangerConfirmChange,
  onPatch,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  active: Conversation | null;
  dangerConfirm: boolean;
  onDangerConfirmChange: (open: boolean) => void;
  onPatch: (patch: { cwd?: string | null; allowDangerous?: boolean }) => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [cwdDraft, setCwdDraft] = useState(active?.cwd ?? '');
  const [picking, setPicking] = useState(false);
  const selectedAgent = active?.agentIds[0] ?? null;
  const approveEffect = autoApproveEffect(selectedAgent);
  const approveEnabled = approveEffect !== 'none';
  const approveOn = autoApproveActive(Boolean(active?.allowDangerous), selectedAgent);

  useEffect(() => {
    setCwdDraft(active?.cwd ?? '');
  }, [active?.id, active?.cwd]);

  function commitCwd(raw: string) {
    const v = raw.trim();
    onPatch({ cwd: v || null });
  }

  async function handleBrowse() {
    setPicking(true);
    try {
      const picked = await pickDirectory({
        title: t('chat.settings.pickDirTitle'),
        defaultPath: cwdDraft || active?.cwd || null,
      });
      if (picked) {
        setCwdDraft(picked);
        onPatch({ cwd: picked });
      }
    } catch (e) {
      toast({
        title: t('chat.settings.pickDirFailed'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setPicking(false);
    }
  }

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('chat.settings.title')}</DialogTitle>
            <DialogDescription>{t('chat.settings.description')}</DialogDescription>
          </DialogHeader>
          {active && (
            <div className="space-y-4 py-2">
              <div>
                <label className="mb-1.5 flex items-center gap-1.5 text-meta text-muted">
                  <FolderOpen className="h-3.5 w-3.5" />
                  {t('chat.settings.cwd')}
                </label>
                <div className="flex gap-2">
                  <Input
                    value={cwdDraft}
                    placeholder={t('chat.settings.cwdPlaceholder')}
                    aria-label={t('chat.settings.cwd')}
                    onChange={(e) => setCwdDraft(e.target.value)}
                    onBlur={(e) => commitCwd(e.target.value)}
                  />
                  <Button
                    type="button"
                    variant="outline"
                    disabled={picking}
                    onClick={() => void handleBrowse()}
                  >
                    {picking ? t('chat.settings.picking') : t('chat.settings.pickDir')}
                  </Button>
                </div>
              </div>
              <label className="flex items-center justify-between gap-3 text-body">
                <span>
                  <span className="block font-medium">{t('chat.settings.autoApprove')}</span>
                  <Tip
                    className="text-meta text-muted"
                    label={
                      approveEnabled
                        ? t('chat.settings.autoApproveOffHint')
                        : autoApproveHint(t, 'none')
                    }
                  >
                    {autoApproveHint(t, approveEffect)}
                  </Tip>
                </span>
                <Switch
                  checked={approveOn}
                  disabled={!approveEnabled}
                  onCheckedChange={(checked) => {
                    if (!approveEnabled) return;
                    if (checked) {
                      onDangerConfirmChange(true);
                      return;
                    }
                    onPatch({ allowDangerous: false });
                  }}
                />
              </label>
            </div>
          )}
          <DialogFooter>
            <Button variant="secondary" onClick={() => onOpenChange(false)}>
              {t('chat.settings.done')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={dangerConfirm} onOpenChange={onDangerConfirmChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('chat.settings.enableTitle')}</DialogTitle>
            <DialogDescription>
              {autoApproveConfirmCopy(t, approveEffect)}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => onDangerConfirmChange(false)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                onDangerConfirmChange(false);
                onPatch({ allowDangerous: true });
              }}
            >
              {t('chat.settings.confirmEnable')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
