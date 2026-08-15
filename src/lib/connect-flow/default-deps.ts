/**
 * ConnectFlow 默认依赖：组装既有 lib/api 门面与本目录纯函数实现。
 */
import { switchAccount } from '@/lib/api/account';
import { listAdapterProfiles } from '@/lib/api/adapter';
import * as providerApi from '@/lib/api/provider';
import {
  bindTicket,
  isActiveBindingForAgent,
  planTicket,
  ticketIdFor,
} from '@/lib/api/tickets';
import type {
  AdapterApplyPlan,
  AdapterApplyRequest,
  AdapterApplyResult,
  AdapterRouteRequest,
} from '@/lib/backend/contracts/adapter';
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
  return planTicket(ticketIdFor(request.sourceKind, request.sourceId), request.targetAgentId);
}

/**
 * Confirm step writes via bind. Success is the Agent's active binding,
 * not "go switch the generated provider in the wallet again".
 */
async function bindViaTicket(request: AdapterApplyRequest): Promise<AdapterApplyResult> {
  const ticketId = ticketIdFor(request.sourceKind, request.sourceId);
  const { binding } = await bindTicket(ticketId, request.targetAgentId);
  if (!isActiveBindingForAgent(binding, request.targetAgentId)) {
    throw new Error('绑定未成为该 Agent 的当前连接');
  }
  const profiles = await listAdapterProfiles();
  const profile = binding.profileId
    ? profiles.find((row) => row.id === binding.profileId)
    : profiles.find((row) => (
      row.sourceKind === request.sourceKind
      && row.sourceId === request.sourceId
      && row.targetAgentId === request.targetAgentId
    ));
  if (!profile) {
    throw new Error('绑定已生效，但未找到对应的绑定配置');
  }
  const providers = await providerApi.listProviders(request.targetAgentId);
  const provider = profile.generatedProviderId
    ? providers.find((row) => row.id === profile.generatedProviderId)
    : undefined;
  if (!provider) {
    throw new Error('绑定已生效，但未找到生成连接');
  }
  return { profile, provider };
}

export function createDefaultConnectFlowDeps(): ConnectFlowDeps {
  return {
    plan: planViaTicket,
    apply: bindViaTicket,
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
