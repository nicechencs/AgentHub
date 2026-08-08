import type { DoctorPort } from '@/lib/backend/contracts';
import { unavailableError } from '@/lib/backend/contracts/errors';

/** Mock mode has no doctor IPC; try* return null for agent/env mock fallbacks. */
export function createMockDoctorPort(): DoctorPort {
  return {
    async getDoctorReport() {
      throw unavailableError('getDoctorReport', 'dev:mock 模式无 Tauri doctor');
    },
    async loadDoctorMapped() {
      throw unavailableError('loadDoctorMapped', 'dev:mock 模式无 Tauri doctor');
    },
    async refreshDoctor() {
      throw unavailableError('refreshDoctor', 'dev:mock 模式无 Tauri doctor');
    },
    async tryLoadDoctorMapped() {
      return null;
    },
    async tryRefreshDoctor() {
      return null;
    },
  };
}
