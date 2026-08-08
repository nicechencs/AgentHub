/**
 * 唯一允许 import @tauri-apps/api 并调用 invoke 的底层封装。
 */
import { Channel, invoke as tauriInvoke } from '@tauri-apps/api/core';
import { isTauriApp } from '@/lib/platform';
import { unavailableError } from '@/lib/backend/contracts/errors';

export { Channel };

export function assertTauriRuntime(feature: string): void {
  if (!isTauriApp()) {
    throw unavailableError(
      feature,
      '当前不是 Tauri 桌面运行时；生产前端不会回退 mock。请使用桌面应用，或开发时运行 pnpm dev:mock',
    );
  }
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  assertTauriRuntime(cmd);
  return tauriInvoke<T>(cmd, args);
}
