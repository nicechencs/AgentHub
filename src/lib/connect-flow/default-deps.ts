/**
 * ConnectFlow 默认依赖：组装既有 lib/api 门面与本目录纯函数实现。
 */
import { switchAccount } from '@/lib/api/account';
import { applyAdapter, listAdapterProfiles } from '@/lib/api/adapter';
import { planTicket } from '@/lib/api/tickets';
import * as providerApi from '@/lib/api/provider';
import type { AdapterApplyPlan, AdapterRouteRequest } from '@/lib/backend/contracts/adapter';
import { buildSourceOptions, isOauthIncomplete } from './eligibility';
import { createPlanFanout } from './plan-fanout';
import type { ConnectFlowDeps, PlanFanoutDeps, SourceOption } from './types';

/**
 * 复用 Connections 页切换链（ConnectionList.tsx openSwitch + confirmSwitch）：
 * - account：无 preview API，直接 switchAccount
 * - provider：必须先 switchPreview，再 switchProvider
 */
async function switchNative(option: SourceOption): Promise<void> {
  const agentId = option.agentId;
  if (option.ref.kind === 'account') {
    await switchAccount(agentId, option.ref.id);
    return;
  }
  await providerApi.switchPreview(agentId, option.ref.id);
  await providerApi.switchProvider(agentId, option.ref.id);
}

/** account 无 preview API；provider 只 preview，不 switch。 */
async function previewNative(option: SourceOption) {
  if (option.ref.kind === 'account') return null;
  return providerApi.switchPreview(option.agentId, option.ref.id);
}

/** plan(ticket, agent) via ticket façade; request still uses source kind/id. */
async function planViaTicket(request: AdapterRouteRequest): Promise<AdapterApplyPlan> {
  return planTicket(`${request.sourceKind}:${request.sourceId}`, request.targetAgentId);
}

export function createDefaultConnectFlowDeps(): ConnectFlowDeps {
  return {
    plan: planViaTicket,
    apply: applyAdapter,
    listProfiles: listAdapterProfiles,
    switchNative,
    previewNative,
    buildSourceOptions,
    isOauthIncomplete,
    createPlanFanout(overrides?: Partial<PlanFanoutDeps>) {
      return createPlanFanout({
        plan: overrides?.plan ?? planViaTicket,
        concurrency: overrides?.concurrency,
        isOauthIncomplete: overrides?.isOauthIncomplete ?? isOauthIncomplete,
      });
    },
  };
}
