import { describe, expect, it } from 'vitest';
import {
  createAsyncSubscriptionCoordinator,
  isCurrentSkillsMarketRequest,
} from './useSkills';

describe('skills async request guards', () => {
  it('accepts only the newest market query request', () => {
    expect(isCurrentSkillsMarketRequest('new', 'old', 2, 1)).toBe(false);
    expect(isCurrentSkillsMarketRequest('new', 'new', 2, 1)).toBe(false);
    expect(isCurrentSkillsMarketRequest('new', 'new', 2, 2)).toBe(true);
  });

  it('single-flights watcher subscription and unsubscribes expired resolutions', async () => {
    const resolvers: Array<(unsubscribe: () => void) => void> = [];
    let subscribeCalls = 0;
    const unsubscribed: string[] = [];
    const coordinator = createAsyncSubscriptionCoordinator(() => {
      subscribeCalls += 1;
      return new Promise<() => void>((resolve) => {
        resolvers.push((unsubscribe) => resolve(unsubscribe));
      });
    });
    const handler = () => {};

    // StrictMode-style setup -> cleanup -> setup before the first subscribe resolves.
    coordinator.retain(handler);
    await Promise.resolve();
    coordinator.release();
    coordinator.retain(handler);
    await Promise.resolve();
    expect(subscribeCalls).toBe(2);

    resolvers[0]?.(() => unsubscribed.push('stale'));
    await Promise.resolve();
    await Promise.resolve();
    expect(unsubscribed).toEqual(['stale']);

    resolvers[1]?.(() => unsubscribed.push('live'));
    await Promise.resolve();
    await Promise.resolve();
    coordinator.release();
    expect(unsubscribed).toEqual(['stale', 'live']);
  });
});
