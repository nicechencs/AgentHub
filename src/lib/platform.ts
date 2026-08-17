/**
 * 运行环境探测工具。
 * Backend 实现选择由 app/runtime + Vite 模式决定；
 * 页面不得用 isTauriApp() 在真实 backend 与 mock 之间做 transport 分支。
 * Tauri adapter 内部的 fail-closed 检测见 lib/backend/tauri/invoke.ts。
 */
import { isTauri } from '@tauri-apps/api/core';

export type { HostPlatform, RuntimeInstallChannel } from './platform-detect';
export {
  detectHostPlatform,
  getRuntimeInstallChannel,
  runtimeInstallChannel,
  supportsRuntimeAutoInstall,
} from './platform-detect';

/** @deprecated 使用 contracts/errors；保留兼容旧 import */
export const FEATURE_NOT_WIRED = '功能尚未接入';

/** 是否运行在 Tauri 桌面壳内（浏览器 Vite 为 false） */
export function isTauriApp(): boolean {
  try {
    return isTauri();
  } catch {
    return false;
  }
}

/** 构造「功能尚未接入」错误，禁止假成功 */
export function notWiredError(action?: string): Error {
  if (action) {
    return new Error(`${action}：${FEATURE_NOT_WIRED}`);
  }
  return new Error(FEATURE_NOT_WIRED);
}
