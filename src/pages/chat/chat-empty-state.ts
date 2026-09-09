/**
 * Empty transcript / empty composer copy. Chips still only fill the draft.
 * Queue-only limits sit on a quiet hint, never the primary placeholder.
 */
import type { TranslateFn } from '@/lib/i18n';

export type EmptyTranscriptCopy = {
  headline: string;
  invite: string;
  identity: string;
  startersHint: string;
};

export function emptyTranscriptCopy(
  t: TranslateFn,
  input: { agentLabel: string; projectLabel: string },
): EmptyTranscriptCopy {
  return {
    headline: t('chat.transcript.start'),
    invite: t('chat.transcript.firstMessage', { agent: input.agentLabel }),
    identity: t('chat.transcript.identity', {
      agent: input.agentLabel,
      project: input.projectLabel,
    }),
    startersHint: t('chat.transcript.startersHint'),
  };
}

/** Grok / Kiro / Claude queue after this turn; Codex can add mid-run. */
export function composerShowsQueueOnlyHint(agentId: string | null | undefined): boolean {
  return agentId === 'grok' || agentId === 'kiro' || agentId === 'claude';
}

export function composerInvitePlaceholder(
  t: TranslateFn,
  input: { emptyTranscript: boolean; agentLabel: string },
): string {
  if (input.emptyTranscript && input.agentLabel.trim()) {
    return t('chat.composer.placeholderInvite', { agent: input.agentLabel.trim() });
  }
  return t('chat.composer.placeholder');
}

export function composerCapabilityHint(
  t: TranslateFn,
  input: { agentId: string | null | undefined; sending: boolean },
): string | null {
  if (input.sending || !composerShowsQueueOnlyHint(input.agentId)) return null;
  return t('chat.composer.queueOnlyHint');
}

/** First-use toolbar: keep controls, quiet the secondary labels. */
export function composerCompactSecondary(input: { emptyTranscript: boolean }): boolean {
  return input.emptyTranscript;
}
