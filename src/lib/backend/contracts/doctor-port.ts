import type { AgentStatus, RuntimeDetect } from '@/lib/types';
import type { DoctorReport } from './doctor-types';

export type { DoctorReport };

export interface DoctorMapped {
  report: DoctorReport;
  runtimes: RuntimeDetect[];
  agents: AgentStatus[];
}
