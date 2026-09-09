import { useI18n } from '@/components/shared/LanguageProvider';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { detectHostPlatform } from '@/lib/platform-detect';
import { CHAT_SHORTCUT_ROWS, chatShortcutChord } from './chat-shortcuts';

export function ChatShortcutsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useI18n();
  const platform = detectHostPlatform();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm" data-help="chat-shortcuts-dialog">
        <DialogHeader>
          <DialogTitle>{t('chat.shortcuts.title')}</DialogTitle>
          <DialogDescription>{t('chat.shortcuts.ime')}</DialogDescription>
        </DialogHeader>
        <ul className="space-y-2">
          {CHAT_SHORTCUT_ROWS.map((row) => (
            <li key={row.id} className="flex items-center justify-between gap-3">
              <span className="text-body text-primary">{t(row.actionKey)}</span>
              <kbd className="inline-flex h-5 min-w-5 items-center justify-center rounded-btn border border-border bg-subtle px-1.5 text-meta leading-none text-muted">
                {chatShortcutChord(row.keys, platform)}
              </kbd>
            </li>
          ))}
        </ul>
      </DialogContent>
    </Dialog>
  );
}
