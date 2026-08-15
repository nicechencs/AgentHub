import type { AgentStatus, RuntimeDetect } from '@/lib/types';
import type { DoctorReport } from './doctor-types';

export type { DoctorReport };

export interface DoctorMapped {
  report: DoctorReport;
  runtimes: RuntimeDetect[];
  agents: AgentStatus[];
}

export interface DoctorPort {
  getDoctorReport(force?: boolean): Promise<DoctorReport>;
  loadDoctorMapped(): Promise<DoctorMapped>;
  refreshDoctor(): Promise<DoctorMapped>;
  tryLoadDoctorMapped(): Promise<DoctorMapped | null>;
  tryRefreshDoctor(): Promise<DoctorMapped | null>;
}
