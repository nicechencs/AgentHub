/**
 * Empty transcript / empty composer copy. Chips still only fill the draft.
 * Queue-only limits and Enter/Shift+Enter sit on hover titles, not permanent lines.
 */
import type { TranslateFn } from '@/lib/i18n';

export type EmptyTranscriptCopy = {
  headline: string;
  startersHint: string;
};

export function emptyTranscriptCopy(t: TranslateFn): EmptyTranscriptCopy {
  return {
    headline: t('chat.transcript.start'),
    startersHint: t('chat.transcript.startersHint'),
  };
}

export function emptyStarterChipHint(taskHint: string, startersHint: string): string {
  return `${taskHint} · ${startersHint}`;
}

/** Continuous chat without mid-turn inject. Reads Options.steer, not Agent names. */
export function composerShowsQueueOnlyHint(input: {
  runtimeEnabled?: boolean;
  steer?: boolean;
}): boolean {
  return input.runtimeEnabled === true && input.steer === false;
}

export function composerInvitePlaceholder(
  t: TranslateFn,
  input: { emptyTranscript: boolean },
): string {
  if (input.emptyTranscript) return t('chat.composer.placeholderInvite');
  return t('chat.composer.placeholder');
}

export function composerCapabilityHint(
  t: TranslateFn,
  input: { runtimeEnabled?: boolean; steer?: boolean; sending: boolean },
): string | null {
  if (input.sending || !composerShowsQueueOnlyHint(input)) return null;
  return t('chat.composer.queueOnlyHint');
}

/** First-use toolbar: keep controls, quiet the secondary labels. */
export function composerCompactSecondary(input: { emptyTranscript: boolean }): boolean {
  return input.emptyTranscript;
}

/** Permanent Enter / queue line stays off the empty session. */
export function composerShowsHintRow(input: { emptyTranscript: boolean }): boolean {
  return !input.emptyTranscript;
}

export function composerHoverHint(shortcut: string, capability: string | null): string {
  return capability ? `${shortcut} ${capability}` : shortcut;
}

export function composerConnectionTooltip(input: {
  label: string;
  subtitle: string | null;
  caption: string | null;
}): string {
  const parts = [input.label.trim()].filter(Boolean);
  const subtitle = input.subtitle?.trim() ?? '';
  if (subtitle && subtitle !== parts[0]) parts.push(subtitle);
  const caption = input.caption?.trim() ?? '';
  if (caption && !parts.includes(caption)) parts.push(caption);
  return parts.join(' · ');
}
