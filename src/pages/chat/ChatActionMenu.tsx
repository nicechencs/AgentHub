import { useLayoutEffect, useState, type RefObject } from 'react';
import { createPortal } from 'react-dom';
import { useI18n } from '@/components/shared/LanguageProvider';
import { cn } from '@/lib/utils';
import {
  chatActionDisabledReason,
  filterChatActions,
  slashMenuFixedPosition,
  type ChatActionContext,
  type ChatActionDef,
  type ChatActionDisableReason,
} from './chat-actions';

/** Slash `/` command palette. Anchored to the composer caret/textarea, not the empty-state. */
export function ChatActionMenu(props: {
  draft: string;
  commandOpen: boolean;
  selectedIndex?: number;
  actionContext: ChatActionContext;
  extraActions?: ChatActionDef[];
  onRun: (action: ChatActionDef) => void;
  onHoverIndex?: (index: number) => void;
  /** Textarea that holds `/`; the panel sits tight above this box. */
  anchorRef: RefObject<HTMLTextAreaElement | null>;
}) {
  const { t } = useI18n();
  const label = (action: ChatActionDef) => action.label ?? t(`chat.actions.${action.labelKey}` as never);
  const disabledCopy = (reason: ChatActionDisableReason) =>
    t(`chat.actions.disabled.${reason}` as never);

  const slashItems = filterChatActions(props.draft, props.extraActions ?? []);
  const selectedIndex = props.selectedIndex ?? 0;
  const commandOpen = props.commandOpen && slashItems.length > 0;
  const [palettePos, setPalettePos] = useState<{ left: number; bottom: number } | null>(null);

  useLayoutEffect(() => {
    if (!commandOpen) {
      setPalettePos(null);
      return;
    }
    const update = () => {
      const el = props.anchorRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      setPalettePos(slashMenuFixedPosition({
        anchorTop: rect.top,
        anchorLeft: rect.left,
        viewportHeight: window.innerHeight,
      }));
    };
    update();
    window.addEventListener('resize', update);
    window.addEventListener('scroll', update, true);
    return () => {
      window.removeEventListener('resize', update);
      window.removeEventListener('scroll', update, true);
    };
  }, [commandOpen, props.anchorRef]);

  if (!commandOpen || !palettePos) return null;

  return createPortal(
    <div
      className="fixed z-50 max-h-56 w-72 overflow-auto rounded-card border border-border bg-panel p-1 shadow-md"
      style={{ left: palettePos.left, bottom: palettePos.bottom }}
      role="listbox"
      aria-label={t('chat.shortcuts.actions')}
      data-help="chat-slash-menu"
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
              active ? 'bg-accent text-accent-foreground' : 'hover:bg-hover',
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
    </div>,
    document.body,
  );
}
