import type { TurnGroup } from './chat-format';

export const OUTLINE_MAGNIFY_RADIUS = 3;
export const OUTLINE_MIN_PROMPTS = 2;
export const OUTLINE_MIN_PANEL_WIDTH_PX = 720;
export const OUTLINE_READING_LINE_PX = 8;
export const OUTLINE_PREVIEW_LIMIT = 120;

export const OUTLINE_TICK = {
  restWidth: 10,
  restHeight: 2,
  activeWidth: 18,
  magnifiedWidth: 26,
  magnifiedHeight: 4,
} as const;

export type ChatOutlinePrompt = {
  id: string;
  preview: string;
};

/**
 * Dock-style falloff: 1 under the pointer, easing to 0 at the radius. The raised cosine
 * has no corner at either end, so sweeping the rail reads as one bulge travelling with
 * the pointer rather than a band switching on and off.
 */
export function promptTickMagnification(slotDistance: number): number {
  const distance = Math.abs(slotDistance);
  if (!Number.isFinite(distance) || distance >= OUTLINE_MAGNIFY_RADIUS) {
    return 0;
  }
  return (1 + Math.cos((Math.PI * distance) / OUTLINE_MAGNIFY_RADIUS)) / 2;
}

export function outlinePromptPreview(text: string, limit = OUTLINE_PREVIEW_LIMIT): string {
  const collapsed = text.replace(/\s+/g, ' ').trim();
  if (collapsed.length <= limit) return collapsed;
  return `${collapsed.slice(0, limit)}…`;
}

export function outlinePromptsFromTurns(turns: readonly TurnGroup[]): ChatOutlinePrompt[] {
  const prompts: ChatOutlinePrompt[] = [];
  for (const group of turns) {
    if (!group.user) continue;
    prompts.push({
      id: group.user.id,
      preview: outlinePromptPreview(group.user.content),
    });
  }
  return prompts;
}

export function shouldShowChatOutline(input: {
  enabled: boolean;
  promptCount: number;
  panelWidth: number;
}): boolean {
  return (
    input.enabled &&
    input.promptCount >= OUTLINE_MIN_PROMPTS &&
    outlinePanelWidthReady(input.panelWidth) &&
    input.panelWidth >= OUTLINE_MIN_PANEL_WIDTH_PX
  );
}

/** 0 / NaN means “not measured yet”. Do not treat an empty host as a mounted rail. */
export function outlinePanelWidthReady(width: number | null | undefined): width is number {
  return typeof width === 'number' && Number.isFinite(width) && width > 0;
}

/** Chat stage (the wide panel), not the inner adaptive content column. */
export const OUTLINE_PANEL_SELECTOR = '[data-chat-stage]';

export function outlinePanelElement(node: Element | null): Element | null {
  if (!node) return null;
  return node.closest(OUTLINE_PANEL_SELECTOR) ?? node;
}

export function readOutlinePanelWidth(node: Element | null): number {
  const panel = outlinePanelElement(node);
  if (!panel) return 0;
  const width = panel.getBoundingClientRect().width;
  return Number.isFinite(width) ? width : 0;
}

export function outlineTickSize(isActive: boolean, magnification: number): {
  width: number;
  height: number;
} {
  const restingWidth = isActive ? OUTLINE_TICK.activeWidth : OUTLINE_TICK.restWidth;
  return {
    width: restingWidth + magnification * (OUTLINE_TICK.magnifiedWidth - restingWidth),
    height:
      OUTLINE_TICK.restHeight +
      magnification * (OUTLINE_TICK.magnifiedHeight - OUTLINE_TICK.restHeight),
  };
}

/** Last prompt whose top edge sits on or above the reading line. */
export function resolveActivePromptId(
  prompts: readonly ChatOutlinePrompt[],
  tops: readonly number[],
  readingLine: number,
): string | null {
  let activeId: string | null = null;
  for (let i = 0; i < prompts.length; i += 1) {
    const top = tops[i];
    if (top == null || !Number.isFinite(top)) continue;
    if (top <= readingLine) activeId = prompts[i]!.id;
  }
  return activeId;
}

export function planOutlineJumpScroll(
  containerTop: number,
  containerScrollTop: number,
  targetTop: number,
  offsetPx = OUTLINE_READING_LINE_PX,
): number {
  return containerScrollTop + (targetTop - containerTop) - offsetPx;
}

export function applyOutlineJumpOffset(
  container: { getBoundingClientRect(): DOMRect; scrollTop: number },
  target: { getBoundingClientRect(): DOMRect },
): void {
  container.scrollTop = planOutlineJumpScroll(
    container.getBoundingClientRect().top,
    container.scrollTop,
    target.getBoundingClientRect().top,
  );
}
