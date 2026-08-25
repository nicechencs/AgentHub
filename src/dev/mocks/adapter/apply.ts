import type {
  AdapterApplyPlan,
  AdapterApplyRequest,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { Provider } from '@/lib/types';
import { materializeFromPlan } from './project';

export function materializeApply(
  request: AdapterApplyRequest,
  plan: AdapterApplyPlan,
  existing: AdapterProfile | undefined,
  now: string,
): { profile: AdapterProfile; provider: Provider } {
  return materializeFromPlan(request, plan, existing, now);
}
