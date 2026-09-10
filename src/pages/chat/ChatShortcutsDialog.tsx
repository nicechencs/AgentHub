import { useI18n } from '@/components/shared/LanguageProvider';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { ChatShortcutOverview } from './ChatShortcutOverview';

export function ChatShortcutsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useI18n();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm" data-help="chat-shortcuts-dialog">
        <DialogHeader>
          <DialogTitle>{t('chat.shortcuts.title')}</DialogTitle>
          <DialogDescription>{t('chat.shortcuts.ime')}</DialogDescription>
        </DialogHeader>
        <ChatShortcutOverview />
      </DialogContent>
    </Dialog>
  );
}
