import type { RuntimeDetect, RuntimeId } from '@/lib/types';
import type { InstallOutcome } from './install-types';

export interface EnvPort {
  listRuntimes(): Promise<RuntimeDetect[]>;
  getRuntime(id: RuntimeId): Promise<RuntimeDetect>;
  installRuntime(id: RuntimeId, channel?: string): Promise<RuntimeDetect>;
  installRuntimeDetailed(id: RuntimeId, channel?: string): Promise<InstallOutcome>;
  installRuntimesBatch(targets: RuntimeId[], channel?: string): Promise<RuntimeDetect[]>;
}
