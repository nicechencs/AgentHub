import type { Backend, EnvPort } from '@/lib/backend/contracts';
import { RuntimeInstallFailedError } from '@/lib/backend/contracts/agent-errors';
import type { RuntimeDetect } from '@/lib/types';
import { invoke } from './invoke';

export { RuntimeInstallFailedError };

export function createTauriEnvPort(backend: Backend): EnvPort {
  return {
    async listRuntimes() {
      const doctor = await backend.doctor.loadDoctorMapped();
      return doctor.runtimes;
    },

    async checkRuntimeUpdates(runtimeIds, force = false) {
      return invoke('check_runtime_updates', {
        runtimeIds: runtimeIds?.length ? runtimeIds : null,
        force,
      });
    },

    async getRuntime(id) {
      const doctor = await backend.doctor.loadDoctorMapped();
      const found = doctor.runtimes.find((r) => r.id === id);
      if (!found) throw new Error(`未知 runtime: ${id}`);
      return found;
    },

    async installRuntimeDetailed(id, channel) {
      return backend.install.installRuntime(id, channel);
    },

    async installRuntime(id, channel) {
      const outcome = await backend.install.installRuntime(id, channel);
      if (!outcome.ok) throw new RuntimeInstallFailedError(outcome);
      await backend.doctor.refreshDoctor();
      return this.getRuntime(id);
    },

    async installRuntimesBatch(targets, channel) {
      const results: RuntimeDetect[] = [];
      const ordered = [...new Set(targets)].sort((a, b) => {
        if (a === 'nodejs') return -1;
        if (b === 'nodejs') return 1;
        return 0;
      });
      for (const id of ordered) {
        results.push(await this.installRuntime(id, channel));
      }
      return results;
    },
  };
}
