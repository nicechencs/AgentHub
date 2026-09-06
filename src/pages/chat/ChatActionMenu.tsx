import { MoreHorizontal } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { CHAT_ACTIONS, filterChatActions, type ChatActionDef } from './chat-actions';

export function ChatActionMenu(props: {
  draft: string;
  commandOpen: boolean;
  onRun: (action: ChatActionDef) => void;
}) {
  const { t } = useI18n();
  const label = (key: string) => t(`chat.actions.${key}` as never);

  const slashItems = filterChatActions(props.draft);

  return (
    <div className="relative">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button type="button" variant="ghost" size="icon" aria-label={t('chat.actions.menu')}>
            <MoreHorizontal className="size-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="min-w-48">
          {CHAT_ACTIONS.map((action) => (
            <DropdownMenuItem key={action.id} onSelect={() => props.onRun(action)}>
              {label(action.labelKey)}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      {props.commandOpen && slashItems.length > 0 ? (
        <div
          className="absolute bottom-full left-0 z-20 mb-2 max-h-56 w-64 overflow-auto rounded-md border bg-popover p-1 shadow-md"
          role="listbox"
          aria-label={t('chat.actions.menu')}
        >
          {slashItems.map((action) => (
            <button
              key={action.id}
              type="button"
              className="flex w-full rounded-sm px-2 py-1.5 text-left text-sm hover:bg-accent"
              onClick={() => props.onRun(action)}
            >
              {label(action.labelKey)}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
