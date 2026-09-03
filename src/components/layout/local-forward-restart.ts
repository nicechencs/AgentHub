import type { LocalForwardLifecyclePhase } from '@/lib/backend/tauri/local-forward-events';

export type LocalForwardRestartBannerInput = {
  restarting?: boolean;
  phase?: LocalForwardLifecyclePhase | null;
  startingComingBack?: boolean;
};

/** Yellow restart banner: status flag, lifecycle event, or starting-coming-back. */
export function localForwardRestartBannerVisible(
  input: LocalForwardRestartBannerInput,
): boolean {
  if (input.phase === 'ready') {
    return Boolean(input.restarting || input.startingComingBack);
  }
  return Boolean(
    input.restarting
    || input.phase === 'restarting'
    || input.startingComingBack,
  );
}

/** Listeners are coming back after a drop; not a user stop. */
export function localForwardStartingComingBack(status: {
  running?: boolean;
  restarting?: boolean;
  statuses?: readonly { state?: string }[];
}): boolean {
  if (status.running) return false;
  if (status.restarting) return true;
  return (status.statuses ?? []).some((row) => row.state === 'starting');
}
