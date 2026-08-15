import { describe, expect, it, vi } from 'vitest';
import { AdapterCommandError } from '@/lib/backend/contracts/adapter';
import type { Account } from '@/lib/types';
import type { AdapterApplyPlan, AdapterRouteAnalysis } from '@/lib/api/adapter';
import { createPlanFanout } from './plan-fanout';
import { OAUTH_INCOMPLETE_MESSAGE } from './eligibility';
import { planFanoutKey, type PlanFanoutRequest } from './types';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function analysis(overrides: Partial<AdapterRouteAnalysis> = {}): AdapterRouteAnalysis {
  return {
    route: 'native_endpoint',
    support: 'stable',
    reason: 'ok',
    actions: [],
    limitations: [],
    evidence: [],
    ...overrides,
  };
}

function plan(overrides: Partial<AdapterApplyPlan> = {}): AdapterApplyPlan {
  return {
    analysis: analysis(),
    targetAgentId: 'claude',
    canApply: true,
    serviceImpact: 'none',
    changes: [],
    ...overrides,
  };
}

function request(overrides: Partial<PlanFanoutRequest> = {}): PlanFanoutRequest {
  return {
    source: { kind: 'provider', id: 'src-1' },
    targetAgentId: 'claude',
    ...overrides,
  };
}

function account(overrides: Partial<Account> = {}): Account {
  return {
    id: 'acc-oauth',
    agentId: 'codex',
    kind: 'oauth',
    label: 'codex@openai',
    isCurrent: false,
    tokenValid: true,
    authHealth: 'renewable',
    ...overrides,
  };
}

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe('createPlanFanout', () => {
  it('serves cached plans on a later start without calling plan again', async () => {
    const planFn = vi.fn(async (route) => plan({
      targetAgentId: route.targetAgentId,
      analysis: analysis({ reason: `${route.sourceId}->${route.targetAgentId}` }),
    }));
    const fanout = createPlanFanout({ plan: planFn });
    const reqA = request({ source: { kind: 'provider', id: 'a' }, targetAgentId: 'claude' });
    const reqB = request({ source: { kind: 'provider', id: 'b' }, targetAgentId: 'claude' });
    const reqAOtherTarget = request({ source: { kind: 'provider', id: 'a' }, targetAgentId: 'codex' });

    fanout.start([reqA]);
    await flush();
    expect(planFn).toHaveBeenCalledOnce();
    expect(fanout.getState().get(planFanoutKey(reqA))?.kind).toBe('ready');

    // 不同请求集合才能走出缓存分支（同集合会被幂等短路）
    fanout.start([reqA, reqB]);
    await flush();
    expect(planFn).toHaveBeenCalledTimes(2);
    expect(fanout.getState().get(planFanoutKey(reqA))?.kind).toBe('ready');
    expect(fanout.getState().get(planFanoutKey(reqB))?.kind).toBe('ready');
    const cachedA = fanout.getState().get(planFanoutKey(reqA));
    expect(cachedA?.kind === 'ready' && cachedA.plan.analysis.reason).toBe('a->claude');

    // 同来源不同 target 必须隔离，不得复用 A→claude 的缓存
    expect(planFanoutKey(reqA)).not.toBe(planFanoutKey(reqAOtherTarget));
    fanout.start([reqA, reqAOtherTarget]);
    await flush();
    expect(planFn).toHaveBeenCalledTimes(3);
    expect(fanout.getState().get(planFanoutKey(reqA))?.kind).toBe('ready');
    const other = fanout.getState().get(planFanoutKey(reqAOtherTarget));
    expect(other?.kind === 'ready' && other.plan.analysis.reason).toBe('a->codex');
  });

  it('keys cache by (kind, id, target) so account/provider id collisions stay distinct', async () => {
    const planFn = vi.fn(async (route) => plan({
      targetAgentId: route.targetAgentId,
      analysis: analysis({ reason: `${route.sourceKind}:${route.sourceId}->${route.targetAgentId}` }),
    }));
    const fanout = createPlanFanout({ plan: planFn });
    const accountReq = request({ source: { kind: 'account', id: 'same-id' }, targetAgentId: 'claude' });
    const providerReq = request({ source: { kind: 'provider', id: 'same-id' }, targetAgentId: 'claude' });
    fanout.start([accountReq, providerReq]);
    await flush();
    expect(planFn).toHaveBeenCalledTimes(2);
    expect(planFanoutKey(accountReq)).not.toBe(planFanoutKey(providerReq));
    const accountState = fanout.getState().get(planFanoutKey(accountReq));
    const providerState = fanout.getState().get(planFanoutKey(providerReq));
    expect(accountState?.kind === 'ready' && accountState.reason).toBeUndefined();
    expect(accountState?.kind === 'ready' && accountState.plan.analysis.reason).toBe('account:same-id->claude');
    expect(providerState?.kind === 'ready' && providerState.plan.analysis.reason).toBe('provider:same-id->claude');
  });

  it('caps concurrency at 3 and dedupes the same key in one start', async () => {
    const slots = Array.from({ length: 5 }, () => deferred<AdapterApplyPlan>());
    let calls = 0;
    const planFn = vi.fn(() => {
      const slot = slots[calls] ?? deferred<AdapterApplyPlan>();
      calls += 1;
      return slot.promise;
    });
    const fanout = createPlanFanout({ plan: planFn, concurrency: 3 });
    const requests = [
      request({ source: { kind: 'provider', id: 'a' } }),
      request({ source: { kind: 'provider', id: 'a' } }),
      request({ source: { kind: 'provider', id: 'b' } }),
      request({ source: { kind: 'provider', id: 'c' } }),
      request({ source: { kind: 'provider', id: 'd' } }),
    ];
    fanout.start(requests);
    await flush();
    expect(planFn).toHaveBeenCalledTimes(3);

    slots[0]!.resolve(plan());
    await flush();
    expect(planFn).toHaveBeenCalledTimes(4);
  });

  it('invalidate clears cache and in-flight so a reopen refetches', async () => {
    const planFn = vi.fn(async () => plan());
    const fanout = createPlanFanout({ plan: planFn });
    const req = request();
    fanout.start([req]);
    await flush();
    expect(planFn).toHaveBeenCalledOnce();

    fanout.invalidate();
    expect(fanout.getState().size).toBe(0);

    fanout.start([req]);
    await flush();
    expect(planFn).toHaveBeenCalledTimes(2);
  });

  it('drops stale responses after a newer start (generation)', async () => {
    const first = deferred<AdapterApplyPlan>();
    const second = deferred<AdapterApplyPlan>();
    let calls = 0;
    const planFn = vi.fn(() => {
      calls += 1;
      return calls === 1 ? first.promise : second.promise;
    });
    const fanout = createPlanFanout({ plan: planFn });
    const firstReq = request({ source: { kind: 'provider', id: 'old' } });
    const secondReq = request({ source: { kind: 'provider', id: 'new' } });

    fanout.start([firstReq]);
    await flush();
    fanout.start([secondReq]);
    first.resolve(plan({ analysis: analysis({ reason: 'stale-old' }) }));
    await flush();
    expect(fanout.getState().has(planFanoutKey(firstReq))).toBe(false);
    expect(fanout.getState().get(planFanoutKey(secondReq))?.kind).toBe('loading');

    second.resolve(plan({ analysis: analysis({ reason: 'fresh-new' }) }));
    await flush();
    const ready = fanout.getState().get(planFanoutKey(secondReq));
    expect(ready?.kind === 'ready' && ready.plan.analysis.reason).toBe('fresh-new');
  });

  it('retry clears one key and refetches only that request', async () => {
    const planFn = vi.fn(async (route) => plan({
      analysis: analysis({ reason: `n=${planFn.mock.calls.length}:${route.sourceId}` }),
    }));
    const fanout = createPlanFanout({ plan: planFn });
    const keep = request({ source: { kind: 'provider', id: 'keep' } });
    const retryReq = request({ source: { kind: 'provider', id: 'retry' } });
    fanout.start([keep, retryReq]);
    await flush();
    expect(planFn).toHaveBeenCalledTimes(2);

    fanout.retry(retryReq);
    await flush();
    expect(planFn).toHaveBeenCalledTimes(3);
    const retried = fanout.getState().get(planFanoutKey(retryReq));
    expect(retried?.kind === 'ready' && retried.plan.analysis.reason).toBe('n=3:retry');
    const kept = fanout.getState().get(planFanoutKey(keep));
    expect(kept?.kind === 'ready' && kept.plan.analysis.reason).toBe('n=1:keep');
  });

  it('prechecks incomplete OAuth and does not send a plan request', async () => {
    const planFn = vi.fn(async () => plan());
    const fanout = createPlanFanout({ plan: planFn });
    const req = request({ source: { kind: 'account', id: 'acc-oauth' }, targetAgentId: 'claude' });
    fanout.start([req], {
      accounts: [account({ id: 'acc-oauth', authHealth: 'needs_login' })],
    });
    await flush();
    expect(planFn).not.toHaveBeenCalled();
    expect(fanout.getState().get(planFanoutKey(req))).toEqual({
      kind: 'blocked_oauth',
      message: OAUTH_INCOMPLETE_MESSAGE,
    });
  });

  it('surfaces Error / string / unknown messages verbatim', async () => {
    const cases: Array<{ thrown: unknown; message: string; id: string }> = [
      { thrown: new Error('plan exploded'), message: 'plan exploded', id: 'err' },
      { thrown: 'legacy string error', message: 'legacy string error', id: 'str' },
      {
        thrown: new AdapterCommandError({
          code: 'not_found',
          message: 'provider not found: missing-1',
          retryable: false,
        }),
        message: 'provider not found: missing-1',
        id: 'cmd',
      },
      { thrown: { message: 'object message' }, message: 'object message', id: 'obj' },
    ];

    for (const item of cases) {
      const fanout = createPlanFanout({
        plan: async () => {
          throw item.thrown;
        },
      });
      const req = request({ source: { kind: 'provider', id: item.id } });
      fanout.start([req]);
      await flush();
      expect(fanout.getState().get(planFanoutKey(req))).toEqual({
        kind: 'error',
        message: item.message,
      });
    }
  });

  it('returns a new immutable snapshot on every change (useSyncExternalStore contract)', async () => {
    const planFn = vi.fn(async () => plan());
    const fanout = createPlanFanout({ plan: planFn });
    const req = request();
    const before = fanout.getState();
    fanout.start([req]);
    const loading = fanout.getState();
    expect(loading).not.toBe(before);
    await flush();
    const ready = fanout.getState();
    // 引用必须变化，否则 useSyncExternalStore 的 Object.is 比较会吞掉更新
    expect(ready).not.toBe(loading);
    // 旧快照不被原地改写
    expect(loading.get(planFanoutKey(req))?.kind).toBe('loading');
    expect(ready.get(planFanoutKey(req))?.kind).toBe('ready');
  });

  it('repeated start with the same request set keeps in-flight work alive (idempotent)', async () => {
    const pending = deferred<AdapterApplyPlan>();
    const planFn = vi.fn(() => pending.promise);
    const fanout = createPlanFanout({ plan: planFn });
    const req = request();
    fanout.start([req]);
    await flush();
    // 模拟 React effect 依赖抖动：同一集合反复声明不得打断在途请求
    fanout.start([req]);
    fanout.start([req]);
    await flush();
    expect(planFn).toHaveBeenCalledOnce();
    pending.resolve(plan());
    await flush();
    expect(fanout.getState().get(planFanoutKey(req))?.kind).toBe('ready');
  });

  it('rebuilds when OAuth gate flips for the same request keys', async () => {
    const planFn = vi.fn(async () => plan());
    const fanout = createPlanFanout({ plan: planFn });
    const req = request({ source: { kind: 'account', id: 'acc-oauth' } });
    const incomplete = account({ id: 'acc-oauth', authHealth: 'needs_login' });
    const complete = account({ id: 'acc-oauth', authHealth: 'renewable', tokenValid: true });

    fanout.start([req], { accounts: [incomplete] });
    await flush();
    expect(planFn).not.toHaveBeenCalled();
    expect(fanout.getState().get(planFanoutKey(req))?.kind).toBe('blocked_oauth');

    fanout.start([req], { accounts: [complete] });
    await flush();
    expect(planFn).toHaveBeenCalledOnce();
    expect(fanout.getState().get(planFanoutKey(req))?.kind).toBe('ready');
  });

  it('voids in-flight work when a key becomes OAuth-blocked', async () => {
    const pending = deferred<AdapterApplyPlan>();
    const planFn = vi.fn(() => pending.promise);
    const fanout = createPlanFanout({ plan: planFn });
    const req = request({ source: { kind: 'account', id: 'acc-oauth' } });
    const complete = account({ id: 'acc-oauth', authHealth: 'renewable', tokenValid: true });
    const incomplete = account({ id: 'acc-oauth', authHealth: 'needs_login' });

    fanout.start([req], { accounts: [complete] });
    await flush();
    expect(planFn).toHaveBeenCalledOnce();

    fanout.start([req], { accounts: [incomplete] });
    pending.resolve(plan({ analysis: analysis({ reason: 'should-drop' }) }));
    await flush();
    expect(planFn).toHaveBeenCalledOnce();
    expect(fanout.getState().get(planFanoutKey(req))).toEqual({
      kind: 'blocked_oauth',
      message: OAUTH_INCOMPLETE_MESSAGE,
    });
  });

  it('keeps in-flight keys when start expands the set', async () => {
    const slotA = deferred<AdapterApplyPlan>();
    const slotB = deferred<AdapterApplyPlan>();
    let calls = 0;
    const planFn = vi.fn(() => {
      calls += 1;
      return calls === 1 ? slotA.promise : slotB.promise;
    });
    const fanout = createPlanFanout({ plan: planFn });
    const reqA = request({ source: { kind: 'provider', id: 'a' } });
    const reqB = request({ source: { kind: 'provider', id: 'b' } });

    fanout.start([reqA]);
    await flush();
    expect(planFn).toHaveBeenCalledOnce();

    fanout.start([reqA, reqB]);
    await flush();
    expect(planFn).toHaveBeenCalledTimes(2);
    expect(fanout.getState().get(planFanoutKey(reqA))?.kind).toBe('loading');
    expect(fanout.getState().get(planFanoutKey(reqB))?.kind).toBe('loading');

    slotA.resolve(plan({ analysis: analysis({ reason: 'from-a' }) }));
    await flush();
    const readyA = fanout.getState().get(planFanoutKey(reqA));
    expect(readyA?.kind === 'ready' && readyA.plan.analysis.reason).toBe('from-a');
    expect(fanout.getState().get(planFanoutKey(reqB))?.kind).toBe('loading');

    slotB.resolve(plan({ analysis: analysis({ reason: 'from-b' }) }));
    await flush();
    const readyB = fanout.getState().get(planFanoutKey(reqB));
    expect(readyB?.kind === 'ready' && readyB.plan.analysis.reason).toBe('from-b');
  });

  it('retry voids the previous in-flight request for that key', async () => {
    const first = deferred<AdapterApplyPlan>();
    const second = deferred<AdapterApplyPlan>();
    const third = deferred<AdapterApplyPlan>();
    const slots = [first, second, third];
    let calls = 0;
    const planFn = vi.fn(() => {
      const slot = slots[calls] ?? deferred<AdapterApplyPlan>();
      calls += 1;
      return slot.promise;
    });
    const fanout = createPlanFanout({ plan: planFn });
    const req = request();
    fanout.start([req]);
    await flush();
    fanout.retry(req);
    fanout.retry(req);
    await flush();
    expect(planFn).toHaveBeenCalledTimes(3);

    first.resolve(plan({ analysis: analysis({ reason: 'stale-start' }) }));
    second.resolve(plan({ analysis: analysis({ reason: 'stale-retry' }) }));
    await flush();
    expect(fanout.getState().get(planFanoutKey(req))?.kind).toBe('loading');

    third.resolve(plan({ analysis: analysis({ reason: 'fresh' }) }));
    await flush();
    const ready = fanout.getState().get(planFanoutKey(req));
    expect(ready?.kind === 'ready' && ready.plan.analysis.reason).toBe('fresh');
  });

  it('rejects non-positive concurrency at creation', () => {
    const planFn = vi.fn(async () => plan());
    expect(() => createPlanFanout({ plan: planFn, concurrency: 0 })).toThrow(RangeError);
    expect(() => createPlanFanout({ plan: planFn, concurrency: -1 })).toThrow(RangeError);
    expect(() => createPlanFanout({ plan: planFn, concurrency: Number.NaN })).toThrow(RangeError);
    expect(() => createPlanFanout({ plan: planFn, concurrency: 1.5 })).toThrow(RangeError);
  });

  it('cancel drops in-flight results without clearing cache', async () => {
    const pending = deferred<AdapterApplyPlan>();
    const planFn = vi.fn(() => pending.promise);
    const fanout = createPlanFanout({ plan: planFn });
    const req = request();
    fanout.start([req]);
    await flush();
    fanout.cancel();
    pending.resolve(plan({ analysis: analysis({ reason: 'after-cancel' }) }));
    await flush();
    expect(fanout.getState().get(planFanoutKey(req))?.kind).toBe('loading');

    fanout.start([req]);
    await flush();
    expect(planFn).toHaveBeenCalledTimes(2);
  });
});
