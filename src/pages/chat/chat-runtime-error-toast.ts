/**
 * Decide whether a ChatEvent error should raise a toast.
 *
 * recover_active writes a durable Error event when an in-flight Codex run dies
 * across restart. Replaying that snapshot on Chat remount / conversation switch
 * must not toast — the cancelled message + interrupted hint is the intended UX.
 */

export type RuntimeErrorToastSource = 'historical' | 'live';

const RECOVER_ACTIVE_INTERRUPT =
  'Codex runtime interrupted; start a new turn to continue';

export function isRuntimeInterruptErrorMessage(message: string): boolean {
  const hay = message.toLowerCase();
  return (
    hay.includes('runtime interrupted') ||
    hay.includes('chat.runtime.interrupted') ||
    hay.includes('codex process stopped') ||
    hay.includes('thread is unavailable')
  );
}

/** Snapshot events are live only after this hook instance already has a watermark. */
export function snapshotErrorToastSource(hadSequenceWatermark: boolean): RuntimeErrorToastSource {
  return hadSequenceWatermark ? 'live' : 'historical';
}

export function shouldToastRuntimeError(input: {
  source: RuntimeErrorToastSource;
  message: string;
}): boolean {
  if (input.source === 'historical') return false;
  if (isRuntimeInterruptErrorMessage(input.message)) return false;
  return Boolean(input.message.trim());
}

export const RECOVER_ACTIVE_INTERRUPT_MESSAGE = RECOVER_ACTIVE_INTERRUPT;
