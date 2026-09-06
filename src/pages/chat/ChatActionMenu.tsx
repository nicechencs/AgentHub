import { MoreHorizontal } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';
import {
  CHAT_ACTIONS,
  chatActionDisabledReason,
  filterChatActions,
  type ChatActionContext,
  type ChatActionDef,
  type ChatActionDisableReason,
} from './chat-actions';

export function ChatActionMenu(props: {
  draft: string;
  commandOpen: boolean;
  selectedIndex?: number;
  actionContext: ChatActionContext;
  extraActions?: ChatActionDef[];
  onRun: (action: ChatActionDef) => void;
  onHoverIndex?: (index: number) => void;
}) {
  const { t } = useI18n();
  const label = (action: ChatActionDef) => action.label ?? t(`chat.actions.${action.labelKey}` as never);
  const disabledCopy = (reason: ChatActionDisableReason) =>
    t(`chat.actions.disabled.${reason}` as never);

  const slashItems = filterChatActions(props.draft, props.extraActions ?? []);
  const selectedIndex = props.selectedIndex ?? 0;

  return (
    <div className="relative">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button type="button" variant="ghost" size="icon" aria-label={t('chat.actions.menu')}>
            <MoreHorizontal className="size-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="min-w-56">
          {CHAT_ACTIONS.map((action) => {
            const reason = chatActionDisabledReason(action, props.actionContext);
            return (
              <DropdownMenuItem
                key={action.id}
                disabled={Boolean(reason)}
                onSelect={() => {
                  if (reason) return;
                  props.onRun(action);
                }}
              >
                <span className="flex w-full flex-col gap-0.5">
                  <span>{label(action)}</span>
                  {reason ? (
                    <span className="text-meta text-muted">{disabledCopy(reason)}</span>
                  ) : null}
                </span>
              </DropdownMenuItem>
            );
          })}
        </DropdownMenuContent>
      </DropdownMenu>
      {props.commandOpen && slashItems.length > 0 ? (
        <div
          className="absolute bottom-full left-0 z-20 mb-2 max-h-56 w-72 overflow-auto rounded-card border bg-popover p-1 shadow-md"
          role="listbox"
          aria-label={t('chat.actions.menu')}
        >
          {slashItems.map((action, index) => {
            const reason = chatActionDisabledReason(action, props.actionContext);
            const active = index === selectedIndex;
            return (
              <button
                key={action.id}
                type="button"
                role="option"
                aria-selected={active}
                disabled={Boolean(reason)}
                className={cn(
                  'flex w-full flex-col rounded-btn px-2 py-1.5 text-left text-sm',
                  active ? 'bg-accent' : 'hover:bg-accent/70',
                  reason && 'cursor-not-allowed opacity-60',
                )}
                onMouseEnter={() => props.onHoverIndex?.(index)}
                onClick={() => {
                  if (reason) return;
                  props.onRun(action);
                }}
              >
                <span>{label(action)}</span>
                {action.description ? (
                  <span className="text-meta text-muted">{action.description}</span>
                ) : null}
                {reason ? (
                  <span className="text-meta text-muted">{disabledCopy(reason)}</span>
                ) : null}
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
