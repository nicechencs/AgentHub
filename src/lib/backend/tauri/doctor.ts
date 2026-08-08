import { mapDoctorReport } from '@/lib/api/doctor-map';
import type { DoctorMapped, DoctorPort } from '@/lib/backend/contracts';
import type { DoctorReport } from '@/lib/backend/contracts/doctor-types';
import { invoke } from './invoke';

const DOCTOR_TTL_MS = 30_000;

export function createTauriDoctorPort(): DoctorPort {
  // Per-port cache: each createBackend() gets isolated state (no production
  // reset*ForTests hooks; tests just construct a new backend).
  let inflightMapped: Promise<DoctorMapped> | null = null;
  let cachedMapped: { at: number; value: DoctorMapped } | null = null;

  async function fetchMapped(force: boolean): Promise<DoctorMapped> {
    const report = await invoke<DoctorReport>('get_doctor_report', { force });
    const mapped = mapDoctorReport(report);
    const value: DoctorMapped = { report, ...mapped };
    cachedMapped = { at: Date.now(), value };
    return value;
  }

  return {
    async getDoctorReport(force = false) {
      return invoke<DoctorReport>('get_doctor_report', { force: force });
    },

    async loadDoctorMapped() {
      if (cachedMapped && Date.now() - cachedMapped.at < DOCTOR_TTL_MS) {
        return cachedMapped.value;
      }
      if (!inflightMapped) {
        inflightMapped = fetchMapped(false).finally(() => {
          inflightMapped = null;
        });
      }
      return inflightMapped;
    },

    async refreshDoctor() {
      cachedMapped = null;
      if (inflightMapped) {
        try {
          await inflightMapped;
        } catch {
          /* ignore */
        }
      }
      inflightMapped = fetchMapped(true).finally(() => {
        inflightMapped = null;
      });
      return inflightMapped;
    },

    async tryLoadDoctorMapped() {
      return this.loadDoctorMapped();
    },

    async tryRefreshDoctor() {
      return this.refreshDoctor();
    },
  };
}
