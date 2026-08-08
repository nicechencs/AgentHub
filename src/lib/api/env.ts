/**
 * Runtime / env API façade.
 */
import { getBackend } from '@/app/runtime';
import type { InstallOutcome } from '@/lib/backend/contracts/install-types';
import { resolveAutoInstallPlan, type AutoInstallPlan } from '@/lib/env-plan';
import type { RuntimeDetect, RuntimeId } from '@/lib/types';

export type { AutoInstallPlan };
export { resolveAutoInstallPlan };

export { RuntimeInstallFailedError } from '@/lib/backend/contracts/agent-errors';

export async function listRuntimes(): Promise<RuntimeDetect[]> {
  return getBackend().env.listRuntimes();
}

export async function getRuntime(id: RuntimeId): Promise<RuntimeDetect> {
  return getBackend().env.getRuntime(id);
}

export async function installRuntimeDetailed(
  id: RuntimeId,
  channel: string = 'winget',
): Promise<InstallOutcome> {
  return getBackend().env.installRuntimeDetailed(id, channel);
}

export async function installRuntime(
  id: RuntimeId,
  channel: string = 'winget',
): Promise<RuntimeDetect> {
  return getBackend().env.installRuntime(id, channel);
}

export async function installRuntimesBatch(
  targets: RuntimeId[],
  channel: string = 'winget',
): Promise<RuntimeDetect[]> {
  return getBackend().env.installRuntimesBatch(targets, channel);
}

/** 逐行推送终端输出(供 UI 展示 install logs) — pure helper, not mock data. */
export function streamScriptLines(
  script: string[],
  onProgress: (lines: string[]) => void,
  intervalMs = 320,
  signal?: { cancelled: boolean },
): Promise<void> {
  return new Promise((resolve, reject) => {
    let i = 0;
    const timer = setInterval(() => {
      if (signal?.cancelled) {
        clearInterval(timer);
        reject(new Error('cancelled'));
        return;
      }
      i += 1;
      onProgress(script.slice(0, i));
      if (i >= script.length) {
        clearInterval(timer);
        resolve();
      }
    }, intervalMs);
  });
}
