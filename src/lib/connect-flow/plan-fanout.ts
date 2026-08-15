/**
 * ConnectFlow plan fan-out：并发上限、同 key 去重、会话缓存、per-key token 防竞态。
 * 纯命令式，无 React import；UI 用 useSyncExternalStore(subscribe, getState)。
 *
 * 快照契约：getState() 返回不可变快照，任何状态变更都会产生新 Map 引用，
 * 否则 useSyncExternalStore 的 Object.is 比较会吞掉所有更新。
 */
import type { Account } from '@/lib/types';
import type { AdapterApplyPlan, AdapterRouteRequest } from '@/lib/api/adapter';
import { isOauthIncomplete, OAUTH_INCOMPLETE_MESSAGE, planToEligibility } from './eligibility';
import type {
  PlanEligibility,
  PlanFanoutController,
  PlanFanoutDeps,
  PlanFanoutRequest,
} from './types';
import { planFanoutKey } from './types';

const DEFAULT_CONCURRENCY = 3;

function resolveConcurrency(value: number | undefined): number {
  const concurrency = value ?? DEFAULT_CONCURRENCY;
  if (!Number.isInteger(concurrency) || concurrency < 1) {
    throw new RangeError(`concurrency must be a positive integer, got ${String(concurrency)}`);
  }
  return concurrency;
}

function fanoutErrorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === 'string') return message;
  }
  return String(error);
}

function toRouteRequest(request: PlanFanoutRequest): AdapterRouteRequest {
  return {
    sourceKind: request.source.kind,
    sourceId: request.source.id,
    targetAgentId: request.targetAgentId,
  };
}

function lookupAccount(
  accounts: readonly Account[] | undefined,
  id: string,
): Account | undefined {
  return accounts?.find((account) => account.id === id);
}

function isOauthBlocked(
  request: PlanFanoutRequest,
  accounts: readonly Account[] | undefined,
  oauthCheck: (account: Account) => boolean,
): boolean {
  if (request.source.kind !== 'account') return false;
  const account = lookupAccount(accounts, request.source.id);
  return Boolean(account && oauthCheck(account));
}

/** 签名纳入每个 key 的 OAuth 门禁，避免完成↔未完成时被幂等短路。 */
function startSignature(keys: readonly string[], blockedKeys: readonly string[]): string {
  return `${[...keys].sort().join('|')}#${[...blockedKeys].sort().join('|')}`;
}

export function createPlanFanout(deps: PlanFanoutDeps): PlanFanoutController {
  const concurrency = resolveConcurrency(deps.concurrency);
  const oauthCheck = deps.isOauthIncomplete ?? isOauthIncomplete;
  const cache = new Map<string, AdapterApplyPlan>();
  /** 不可变快照：每次变更换新 Map（见文件头快照契约） */
  let state: ReadonlyMap<string, PlanEligibility> = new Map();
  const listeners = new Set<() => void>();
  const tokens = new Map<string, number>();
  /** 已入队或正在请求的 key；finally 仅在 token 仍匹配时删除 */
  const activeKeys = new Set<string>();
  let running = 0;
  let queue: PlanFanoutRequest[] = [];
  /** start() 幂等保护：同一请求集合 + 同一门禁结果才 no-op */
  let lastSignature: string | null = null;

  const notify = () => {
    listeners.forEach((listener) => listener());
  };

  const setEligibility = (key: string, value: PlanEligibility) => {
    const next = new Map(state);
    next.set(key, value);
    state = next;
  };

  const nextToken = (key: string): number => {
    const token = (tokens.get(key) ?? 0) + 1;
    tokens.set(key, token);
    return token;
  };

  const voidKey = (key: string) => {
    nextToken(key);
    queue = queue.filter((item) => planFanoutKey(item) !== key);
    activeKeys.delete(key);
  };

  const voidAllInFlight = () => {
    for (const key of activeKeys) {
      nextToken(key);
    }
    activeKeys.clear();
    queue = [];
  };

  const enqueueUnique = (request: PlanFanoutRequest) => {
    const key = planFanoutKey(request);
    if (activeKeys.has(key)) return;
    activeKeys.add(key);
    queue.push(request);
  };

  const pump = () => {
    while (running < concurrency && queue.length > 0) {
      const request = queue.shift()!;
      const key = planFanoutKey(request);
      const token = nextToken(key);
      running += 1;
      void deps
        .plan(toRouteRequest(request))
        .then((plan) => {
          if (tokens.get(key) !== token) return;
          cache.set(key, plan);
          setEligibility(key, planToEligibility(plan));
          notify();
        })
        .catch((error: unknown) => {
          if (tokens.get(key) !== token) return;
          setEligibility(key, { kind: 'error', message: fanoutErrorMessage(error) });
          notify();
        })
        .finally(() => {
          if (tokens.get(key) === token) {
            activeKeys.delete(key);
          }
          running -= 1;
          pump();
        });
    }
  };

  const start = (
    requests: readonly PlanFanoutRequest[],
    options?: { accounts?: readonly Account[] },
  ) => {
    const unique: PlanFanoutRequest[] = [];
    const seen = new Set<string>();
    const blockedKeys: string[] = [];
    for (const request of requests) {
      const key = planFanoutKey(request);
      if (seen.has(key)) continue;
      seen.add(key);
      unique.push(request);
      if (isOauthBlocked(request, options?.accounts, oauthCheck)) {
        blockedKeys.push(key);
      }
    }

    const keys = unique.map(planFanoutKey);
    const signature = startSignature(keys, blockedKeys);
    if (signature === lastSignature) return;
    lastSignature = signature;

    const blocked = new Set(blockedKeys);
    const next = new Map<string, PlanEligibility>();
    const pending: PlanFanoutRequest[] = [];

    for (const request of unique) {
      const key = planFanoutKey(request);
      if (blocked.has(key)) {
        voidKey(key);
        next.set(key, { kind: 'blocked_oauth', message: OAUTH_INCOMPLETE_MESSAGE });
        continue;
      }

      const cached = cache.get(key);
      if (cached) {
        next.set(key, planToEligibility(cached));
        continue;
      }

      if (activeKeys.has(key)) {
        next.set(key, { kind: 'loading' });
        continue;
      }

      next.set(key, { kind: 'loading' });
      pending.push(request);
    }

    for (const key of state.keys()) {
      if (!seen.has(key)) voidKey(key);
    }

    state = next;
    for (const request of pending) enqueueUnique(request);
    notify();
    pump();
  };

  return {
    start,
    retry(request) {
      const key = planFanoutKey(request);
      cache.delete(key);
      voidKey(key);
      setEligibility(key, { kind: 'loading' });
      enqueueUnique(request);
      notify();
      pump();
    },
    cancel() {
      voidAllInFlight();
      lastSignature = null;
    },
    invalidate() {
      voidAllInFlight();
      cache.clear();
      state = new Map();
      lastSignature = null;
      notify();
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    getState() {
      return state;
    },
  };
}
