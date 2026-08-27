import { describe, expect, it } from 'vitest';
import type { AdapterApplyPlan } from '@/lib/backend/contracts/adapter';
import type { PlanEligibility } from '@/lib/connect-flow/types';
import { ticketRouteHintFromEligibility } from './use-ticket-route-actions';

function readyPlan(partial: Partial<AdapterApplyPlan> & Pick<AdapterApplyPlan, 'analysis'>): AdapterApplyPlan {
  return {
    targetAgentId: 'codex',
    canApply: false,
    serviceImpact: 'none',
    changes: [],
    ...partial,
  };
}

describe('ticketRouteHintFromEligibility', () => {
  it('maps loading and missing to pending', () => {
    expect(ticketRouteHintFromEligibility(undefined)).toEqual({ status: 'pending' });
    expect(ticketRouteHintFromEligibility({ kind: 'loading' })).toEqual({ status: 'pending' });
  });

  it('keeps oauth and plan reasons', () => {
    expect(ticketRouteHintFromEligibility({
      kind: 'blocked_oauth',
      message: '官方登录未完成，先到连接页完成登录。',
    })).toEqual({
      status: 'blocked_oauth',
      reason: '官方登录未完成，先到连接页完成登录。',
    });

    const ready: PlanEligibility = {
      kind: 'ready',
      canApply: false,
      routeSummary: '本机路由',
      reason: '规则还没做完',
      plan: readyPlan({
        canApply: false,
        reason: '规则还没做完',
        analysis: {
          route: 'local_bridge',
          support: 'experimental',
          reason: '需要本机转发',
          actions: [],
          limitations: [],
          evidence: [],
        },
      }),
    };
    expect(ticketRouteHintFromEligibility(ready)).toEqual({
      status: 'ready',
      route: 'local_bridge',
      canApply: false,
      reason: '规则还没做完',
    });
  });
});
