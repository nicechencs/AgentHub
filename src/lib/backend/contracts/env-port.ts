import type { RuntimeDetect, RuntimeId, RuntimeUpdateInfo } from '@/lib/types';
import type { InstallOutcome } from './install-types';

export interface EnvPort {
  listRuntimes(): Promise<RuntimeDetect[]>;
  /** 从官方版本源检查更新；网络失败以 unknown 返回，force 会绕过缓存。 */
  checkRuntimeUpdates(runtimeIds?: RuntimeId[], force?: boolean): Promise<RuntimeUpdateInfo[]>;
  getRuntime(id: RuntimeId): Promise<RuntimeDetect>;
  installRuntime(id: RuntimeId, channel?: string): Promise<RuntimeDetect>;
  installRuntimeDetailed(id: RuntimeId, channel?: string): Promise<InstallOutcome>;
  installRuntimesBatch(targets: RuntimeId[], channel?: string): Promise<RuntimeDetect[]>;
}
