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
    throw new Error('还没有切到这份登录');
  }
  if (binding.route === 'native' && !binding.profileId) {
    return {
      profile: {
        id: `native:${ticketId}:${request.targetAgentId}`,
        name: '官方登录',
        sourceKind: request.sourceKind,
        sourceId: request.sourceId,
        targetAgentId: request.targetAgentId,
        route: 'native_endpoint',
        mode: 'oauth',
        status: 'active',
        ruleId: 'codex-subscription-to-codex-v1',
        ruleVersion: '1',
        generatedProviderId: null,
        autoStart: false,
        createdAt: '',
        updatedAt: '',
      },
      provider: {
        id: `native:${ticketId}`,
        agentId: request.targetAgentId,
        name: '官方登录',
        preset: 'official',
        configText: '',
        configFormat: 'toml',
        isCurrent: true,
      },
    };
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
    throw new Error('已接上，但找不到对应的本机路由记录');
  }
  const providers = await providerApi.listProviders(request.targetAgentId);
  const provider = profile.generatedProviderId
    ? providers.find((row) => row.id === profile.generatedProviderId)
    : undefined;
  if (!provider) {
    throw new Error('已接上，但找不到写入目标工具的本机地址');
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
