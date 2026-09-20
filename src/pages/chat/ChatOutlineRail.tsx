import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useState,
  type MouseEvent,
  type PointerEvent,
  type RefObject,
} from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { usePrefersReducedMotion } from '@/lib/motion';
import { cn } from '@/lib/utils';
import type { TurnGroup } from './chat-format';
import { createChatOutlineHoverIntent } from './chat-outline-hover';
import {
  OUTLINE_READING_LINE_PX,
  outlinePanelElement,
  outlinePanelWidthReady,
  outlinePromptsFromTurns,
  outlineTickSize,
  promptTickMagnification,
  readOutlinePanelWidth,
  resolveActivePromptId,
  shouldShowChatOutline,
  type ChatOutlinePrompt,
} from './chat-outline-model';
import { loadChatOutlineEnabled } from './chat-outline-pref';

const RAIL_WIDTH_PX = 36;
const SLOT_HEIGHT_PX = 8;
const PREVIEW_GAP_PX = 4;

export function ChatOutlineRail({
  turns,
  scrollRef,
  onJumpToPrompt,
  measuredWidth,
  enabled,
}: {
  turns: TurnGroup[];
  scrollRef?: RefObject<HTMLDivElement>;
  onJumpToPrompt?: (messageId: string) => void;
  measuredWidth?: number;
  enabled?: boolean;
}) {
  const { t } = useI18n();
  const prompts = useMemo(() => outlinePromptsFromTurns(turns), [turns]);
  const [prefEnabled] = useState(loadChatOutlineEnabled);
  const isEnabled = enabled ?? prefEnabled;
  const [observedWidth, setObservedWidth] = useState(0);
  const hasMeasuredWidth = outlinePanelWidthReady(measuredWidth);
  const panelWidth = hasMeasuredWidth ? measuredWidth : observedWidth;
  const [measureNode, setMeasureNode] = useState<HTMLDivElement | null>(null);
  const assignMeasureRef = useCallback((node: HTMLDivElement | null) => {
    setMeasureNode((prev) => (prev === node ? prev : node));
  }, []);
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const [focusedIndex, setFocusedIndex] = useState<number | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const prefersReducedMotion = usePrefersReducedMotion();

  const hoverIntent = useMemo(
    () =>
      createChatOutlineHoverIntent({
        activate: setHoveredIndex,
        schedule: (callback, delayMs) => window.setTimeout(callback, delayMs),
        cancel: (timerId) => window.clearTimeout(timerId),
      }),
    [],
  );

  useEffect(() => () => hoverIntent.dispose(), [hoverIntent]);

  useEffect(() => {
    if (hasMeasuredWidth) return;
    const node = measureNode;
    if (!node) return;
    const apply = () => {
      const next = readOutlinePanelWidth(node);
      setObservedWidth((prev) => (prev === next ? prev : next));
    };
    apply();
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', apply);
      return () => window.removeEventListener('resize', apply);
    }
    const observer = new ResizeObserver(apply);
    observer.observe(outlinePanelElement(node) ?? node);
    return () => observer.disconnect();
  }, [hasMeasuredWidth, measureNode]);

  const readActivePrompt = useCallback(() => {
    const container = scrollRef?.current;
    if (!container) return;
    const readingLine = container.getBoundingClientRect().top + OUTLINE_READING_LINE_PX;
    const tops = prompts.map((prompt) => {
      const el = document.getElementById(`chat-msg-${prompt.id}`);
      return el ? el.getBoundingClientRect().top : Number.POSITIVE_INFINITY;
    });
    setActiveId((current) => {
      const next = resolveActivePromptId(prompts, tops, readingLine);
      return current === next ? current : next;
    });
  }, [prompts, scrollRef]);

  useEffect(() => {
    const container = scrollRef?.current;
    if (!container) return;
    container.addEventListener('scroll', readActivePrompt, { passive: true });
    readActivePrompt();
    return () => container.removeEventListener('scroll', readActivePrompt);
  }, [readActivePrompt, scrollRef]);

  const handlePointerEnterTick = useCallback(
    (index: number) => hoverIntent.pointAt(index),
    [hoverIntent],
  );
  const handlePointerEnterRail = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      hoverIntent.enter({ x: event.clientX, y: event.clientY });
    },
    [hoverIntent],
  );
  const handlePointerMoveRail = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      hoverIntent.move({ x: event.clientX, y: event.clientY });
    },
    [hoverIntent],
  );
  const handlePointerLeaveRail = useCallback(() => hoverIntent.leave(), [hoverIntent]);
  const handleFocusChange = useCallback((index: number, focused: boolean) => {
    setFocusedIndex((current) => {
      if (focused) return index;
      return current === index ? null : current;
    });
  }, []);

  const visible = shouldShowChatOutline({
    enabled: isEnabled,
    promptCount: prompts.length,
    panelWidth,
  });
  const attentionIndex = hoveredIndex ?? focusedIndex;

  useEffect(() => {
    if (!visible) hoverIntent.leave();
  }, [hoverIntent, visible]);

  if (!isEnabled) return null;

  return (
    <div
      ref={assignMeasureRef}
      className="pointer-events-none absolute inset-0"
      data-chat-outline-measure
    >
      {visible ? (
        <div
          role="tablist"
          aria-label={t('chat.outline.aria')}
          data-testid="chat-outline-rail"
          className="pointer-events-auto absolute bottom-[10%] top-[10%] z-[2] flex flex-col items-center justify-center"
          style={{ left: 8, width: RAIL_WIDTH_PX }}
          onPointerEnter={handlePointerEnterRail}
          onPointerMove={handlePointerMoveRail}
          onPointerLeave={handlePointerLeaveRail}
        >
          {prompts.map((prompt, index) => (
            <ChatOutlineTick
              key={prompt.id}
              index={index}
              prompt={prompt}
              label={t('chat.outline.tick', {
                n: index + 1,
                total: prompts.length,
                preview: prompt.preview,
              })}
              isActive={prompt.id === activeId}
              hasAttention={index === attentionIndex}
              magnification={
                prefersReducedMotion || attentionIndex === null
                  ? 0
                  : promptTickMagnification(index - attentionIndex)
              }
              onHover={handlePointerEnterTick}
              onFocusChange={handleFocusChange}
              onJumpToPrompt={onJumpToPrompt}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

const ChatOutlineTick = memo(function ChatOutlineTick({
  index,
  prompt,
  label,
  isActive,
  hasAttention,
  magnification,
  onHover,
  onFocusChange,
  onJumpToPrompt,
}: {
  index: number;
  prompt: ChatOutlinePrompt;
  label: string;
  isActive: boolean;
  hasAttention: boolean;
  magnification: number;
  onHover: (index: number) => void;
  onFocusChange: (index: number, focused: boolean) => void;
  onJumpToPrompt?: (messageId: string) => void;
}) {
  const size = outlineTickSize(isActive, magnification);
  const handleJump = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      onJumpToPrompt?.(prompt.id);
      event.currentTarget.blur();
      onFocusChange(index, false);
    },
    [index, onFocusChange, onJumpToPrompt, prompt.id],
  );

  return (
    <div
      className="relative flex min-h-0 items-center justify-center"
      style={{
        width: RAIL_WIDTH_PX,
        flexBasis: SLOT_HEIGHT_PX,
        flexGrow: 1,
        flexShrink: 1,
      }}
      onPointerEnter={() => onHover(index)}
    >
      <button
        type="button"
        role="tab"
        aria-selected={isActive}
        aria-label={label}
        data-testid={`chat-outline-tick-${prompt.id}`}
        className="flex h-full w-full cursor-pointer items-center justify-start bg-transparent pl-1 outline-none"
        onMouseDown={(event) => event.preventDefault()}
        onClick={handleJump}
        onFocus={() => onFocusChange(index, true)}
        onBlur={() => onFocusChange(index, false)}
      >
        <span
          className={cn(
            'block rounded-full transition-[width,height,background-color] duration-[140ms] ease-out motion-reduce:transition-none',
            hasAttention ? 'bg-primary' : isActive ? 'bg-muted' : 'bg-border',
          )}
          style={{ width: size.width, height: size.height }}
        />
      </button>
      {hasAttention ? (
        <div
          aria-hidden
          data-testid="chat-outline-preview"
          className="pointer-events-none absolute top-1/2 z-[3] h-12 overflow-hidden rounded-card border border-border bg-panel px-3 py-1.5 shadow-md"
          style={{
            left: RAIL_WIDTH_PX + PREVIEW_GAP_PX,
            width: 260,
            marginTop: -24,
          }}
        >
          <p className="line-clamp-2 text-meta text-primary">{prompt.preview}</p>
        </div>
      ) : null}
    </div>
  );
});
