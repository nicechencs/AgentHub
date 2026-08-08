/**
 * Doctor API façade — TTL cache lives in tauri adapter.
 */
import { getBackend } from '@/app/runtime';
import type { DoctorMapped, DoctorReport } from '@/lib/backend/contracts';

export type {
  DoctorAgentId,
  DoctorRuntimeId,
  DoctorEnvStatusKind,
  DoctorDetectStatus,
  DoctorRemediation,
  DoctorEnvStatus,
  DoctorDetectResult,
  DoctorCapabilityState,
  DoctorPathInfo,
  DoctorReport,
} from '@/lib/backend/contracts/doctor-types';
export type { DoctorMapped } from '@/lib/backend/contracts/doctor-port';

export async function getDoctorReport(force = false): Promise<DoctorReport> {
  return getBackend().doctor.getDoctorReport(force);
}

export async function loadDoctorMapped(): Promise<DoctorMapped> {
  return getBackend().doctor.loadDoctorMapped();
}

export async function refreshDoctor(): Promise<DoctorMapped> {
  return getBackend().doctor.refreshDoctor();
}

export async function tryLoadDoctorMapped(): Promise<DoctorMapped | null> {
  return getBackend().doctor.tryLoadDoctorMapped();
}

export async function tryRefreshDoctor(): Promise<DoctorMapped | null> {
  return getBackend().doctor.tryRefreshDoctor();
}
