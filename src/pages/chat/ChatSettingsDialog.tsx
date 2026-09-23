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
import { isKiroChatAgent } from './chat-kiro-model';
import {
  autoApproveActive,
  autoApproveConfirmCopy,
  autoApproveEffect,
  autoApproveHint,
  canRebindConversationCwd,
} from './chat-model';
import {
  agentNewChatConnectKind,
  chatConnectLabelKey,
  sessionChatConnectKind,
} from './chat-connect-model';
import { sessionAllowAlwaysActive } from './chat-runtime-model';
import type { RuntimeChannel, RuntimeSnapshot } from '@/lib/api/chat';

export function ChatSettingsDialog({
  open,
  onOpenChange,
  active,
  dangerConfirm,
  onDangerConfirmChange,
  onPatch,
  runtimeLocked = false,
  turnActive = false,
  transport = null,
  runtimeEnabled = false,
  runtime = null,
  onClearSessionAllowAlways,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  active: Conversation | null;
  dangerConfirm: boolean;
  onDangerConfirmChange: (open: boolean) => void;
  onPatch: (patch: { cwd?: string | null; allowDangerous?: boolean }) => void;
  runtimeLocked?: boolean;
  turnActive?: boolean;
  transport?: RuntimeChannel | null;
  runtimeEnabled?: boolean;
  runtime?: Pick<RuntimeSnapshot, 'sessionAllowAlways'> | null;
  onClearSessionAllowAlways?: () => Promise<void> | void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [cwdDraft, setCwdDraft] = useState(active?.cwd ?? '');
  const [picking, setPicking] = useState(false);
  const selectedAgent = active?.agentIds[0] ?? null;
  const approveEffect = autoApproveEffect(selectedAgent);
  const approveEnabled = approveEffect !== 'none';
  const approveOn = autoApproveActive(Boolean(active?.allowDangerous), selectedAgent);
  const kiroPermissions = isKiroChatAgent(selectedAgent);
  const permissionLocked = kiroPermissions && turnActive;
  const cwdLocked = !canRebindConversationCwd(active ?? { cwd: null }, runtimeLocked);
  const connectKind = sessionChatConnectKind({
    agentId: selectedAgent,
    transport,
    runtimeEnabled,
  });
  const agentConnectKind = agentNewChatConnectKind(selectedAgent);
  const sessionAlways = sessionAllowAlwaysActive(runtime);

  useEffect(() => {
    setCwdDraft(active?.cwd ?? '');
  }, [active?.id, active?.cwd]);

  function commitCwd(raw: string) {
    if (cwdLocked) return;
    const v = raw.trim();
    onPatch({ cwd: v || null });
  }

  async function handleBrowse() {
    if (cwdLocked) return;
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
          <DialogHeader className="mb-2">
            <DialogTitle>{t('chat.settings.title')}</DialogTitle>
            <DialogDescription className="sr-only">{t('chat.settings.description')}</DialogDescription>
          </DialogHeader>
          {active && (
            <div className="space-y-3">
              <div>
                <label className="mb-1 flex items-center gap-1.5 text-meta text-muted">
                  <FolderOpen className="h-3.5 w-3.5" />
                  {t('chat.settings.cwd')}
                </label>
                <div className="flex gap-2">
                  <Input
                    value={cwdDraft}
                    placeholder={t('chat.settings.cwdPlaceholder')}
                    aria-label={t('chat.settings.cwd')}
                    disabled={cwdLocked}
                    title={cwdLocked ? t('chat.runtimeOps.sessionLocked') : undefined}
                    onChange={(e) => setCwdDraft(e.target.value)}
                    onBlur={(e) => commitCwd(e.target.value)}
                  />
                  <Button
                    type="button"
                    variant="outline"
                    disabled={picking || cwdLocked}
                    title={cwdLocked ? t('chat.runtimeOps.sessionLocked') : undefined}
                    onClick={() => void handleBrowse()}
                  >
                    {picking ? t('chat.settings.picking') : t('chat.settings.pickDir')}
                  </Button>
                </div>
              </div>
              <div
                className="flex items-baseline justify-between gap-3"
                data-help="chat-session-connect"
              >
                <span className="text-meta text-muted">{t('chat.connect.sessionTitle')}</span>
                <p className="text-right text-body">
                  {t(chatConnectLabelKey(connectKind))}
                  {agentConnectKind !== connectKind ? (
                    <span className="mt-0.5 block text-meta text-muted">
                      {t('chat.connect.agentTitle')}
                      {' · '}
                      {t(chatConnectLabelKey(agentConnectKind))}
                    </span>
                  ) : null}
                </p>
              </div>
              {connectKind !== 'legacy' ? (
                <label className="flex items-center justify-between gap-3 text-body" data-help="chat-session-always-allow-setting">
                  <span>
                    <span className="block">{t('chat.runtime.sessionRemembered')}</span>
                    <span className="text-meta text-muted">
                      {sessionAlways
                        ? t(
                            kiroPermissions
                              ? 'chat.runtime.sessionRememberedOnHintKiro'
                              : 'chat.runtime.sessionRememberedOnHint',
                          )
                        : t('chat.runtime.sessionRememberedOff')}
                    </span>
                  </span>
                  <Switch
                    checked={sessionAlways}
                    disabled={!sessionAlways}
                    aria-label={t('chat.runtime.sessionRemembered')}
                    title={
                      sessionAlways
                        ? t('chat.runtime.sessionRememberedClear')
                        : t('chat.runtime.sessionRememberedOff')
                    }
                    onCheckedChange={(checked) => {
                      if (checked || !sessionAlways) return;
                      void onClearSessionAllowAlways?.();
                    }}
                  />
                </label>
              ) : null}
              {kiroPermissions ? (
                <fieldset className="space-y-1.5" disabled={permissionLocked}>
                  <legend className="text-meta text-muted">{t('chat.kiro.permissionTitle')}</legend>
                  <label className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 hover:bg-subtle">
                    <input
                      type="radio"
                      name={`kiro-permission-${active.id}`}
                      checked={!approveOn}
                      disabled={permissionLocked}
                      onChange={() => onPatch({ allowDangerous: false })}
                    />
                    <span>
                      {t('chat.kiro.permissionAsk')}
                      <span className="text-meta text-muted">
                        {' · '}
                        {t('chat.kiro.permissionAskHint')}
                      </span>
                    </span>
                  </label>
                  <label className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 hover:bg-subtle">
                    <input
                      type="radio"
                      name={`kiro-permission-${active.id}`}
                      checked={approveOn}
                      disabled={permissionLocked}
                      onChange={() => {
                        if (approveOn || permissionLocked) return;
                        onDangerConfirmChange(true);
                      }}
                    />
                    <span>
                      {t('chat.kiro.permissionFull')}
                      <span className="text-meta text-muted">
                        {' · '}
                        {t('chat.kiro.permissionFullHint')}
                      </span>
                    </span>
                  </label>
                </fieldset>
              ) : (
                <label className="flex items-center justify-between gap-3 text-body">
                  <span>
                    <span className="block">{t('chat.settings.autoApprove')}</span>
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
              )}
            </div>
          )}
          <DialogFooter className="mt-4">
            <Button variant="secondary" onClick={() => onOpenChange(false)}>
              {t('chat.settings.done')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={dangerConfirm} onOpenChange={onDangerConfirmChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {kiroPermissions ? t('chat.kiro.enableFullTitle') : t('chat.settings.enableTitle')}
            </DialogTitle>
            <DialogDescription>
              {autoApproveConfirmCopy(t, approveEffect, selectedAgent)}
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
                if (permissionLocked) return;
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
