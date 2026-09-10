import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Keyboard } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { TOOLTIP } from '@/components/ui/tooltip';
import { ChatShortcutOverview } from './ChatShortcutOverview';
import {
  shortcutsHelpExpanded,
  shortcutsHelpOpenChange,
  type ShortcutsHelpOpenReason,
} from './chat-shortcuts-help';

const HOVER_OPEN_MS = TOOLTIP.delayMs;
const HOVER_CLOSE_MS = 160;

export function ChatShortcutsHelp() {
  const { t } = useI18n();
  const panelId = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const openTimer = useRef<number>();
  const closeTimer = useRef<number>();
  const [reason, setReason] = useState<ShortcutsHelpOpenReason>(null);
  const [pos, setPos] = useState<{ right: number; bottom: number } | null>(null);
  const open = shortcutsHelpExpanded(reason);

  const clearTimers = () => {
    window.clearTimeout(openTimer.current);
    window.clearTimeout(closeTimer.current);
  };

  const apply = (event: Parameters<typeof shortcutsHelpOpenChange>[1]) => {
    setReason((current) => shortcutsHelpOpenChange(current, event));
  };

  useEffect(() => () => clearTimers(), []);

  useLayoutEffect(() => {
    if (!open) {
      setPos(null);
      return;
    }
    const update = () => {
      const el = triggerRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      setPos({
        right: window.innerWidth - rect.right,
        bottom: window.innerHeight - rect.top + 8,
      });
    };
    update();
    window.addEventListener('resize', update);
    window.addEventListener('scroll', update, true);
    return () => {
      window.removeEventListener('resize', update);
      window.removeEventListener('scroll', update, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (triggerRef.current?.contains(target)) return;
      if (panelRef.current?.contains(target)) return;
      apply('dismiss');
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      apply('dismiss');
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [open]);

  return (
    <div
      className="relative shrink-0"
      onMouseEnter={() => {
        window.clearTimeout(closeTimer.current);
        window.clearTimeout(openTimer.current);
        openTimer.current = window.setTimeout(() => apply('hover-enter'), HOVER_OPEN_MS);
      }}
      onMouseLeave={() => {
        window.clearTimeout(openTimer.current);
        closeTimer.current = window.setTimeout(() => apply('hover-leave'), HOVER_CLOSE_MS);
      }}
    >
      <Button
        ref={triggerRef}
        type="button"
        size="icon"
        variant="ghost"
        className="h-7 w-7 text-muted"
        aria-label={t('chat.shortcuts.open')}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={panelId}
        aria-keyshortcuts="?"
        data-help="chat-shortcuts"
        onClick={() => {
          clearTimers();
          apply('click');
        }}
      >
        <Keyboard className="h-3.5 w-3.5" aria-hidden />
      </Button>
      {open && pos && typeof document !== 'undefined'
        ? createPortal(
            <div
              ref={panelRef}
              id={panelId}
              role="dialog"
              data-state="open"
              data-help="chat-shortcuts-popover"
              aria-label={t('chat.shortcuts.overview')}
              className="fixed z-50 w-64 rounded-card border border-border bg-panel p-3 shadow-sm"
              style={{ right: pos.right, bottom: pos.bottom }}
              onMouseEnter={() => {
                window.clearTimeout(closeTimer.current);
                apply('hover-enter');
              }}
              onMouseLeave={() => {
                closeTimer.current = window.setTimeout(() => apply('hover-leave'), HOVER_CLOSE_MS);
              }}
            >
              <p className="mb-2 text-meta text-muted">{t('chat.shortcuts.ime')}</p>
              <ChatShortcutOverview />
            </div>,
            document.body,
          )
        : null}
    </div>
  );
}
