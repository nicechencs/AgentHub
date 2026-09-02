/**
 * Tauri-only live-vs-pool login heal/conflict events.
 * Only Tauri ports (and App shell) may import this module.
 */
import type { ProviderBindingHealPayload } from '@/lib/backend/contracts/provider-heal-types';
import { PROVIDER_BINDING_HEAL_EVENT } from '@/lib/backend/contracts/provider-heal-types';
import { unavailableError } from '@/lib/backend/contracts/errors';
import { isTauriApp } from '@/lib/platform';

export { PROVIDER_BINDING_HEAL_EVENT };
export type { ProviderBindingHealPayload };

type Unlisten = () => void;

let listenStarted = false;
let listenPromise: Promise<Unlisten> | null = null;
let consumer: ((payload: ProviderBindingHealPayload) => void) | null = null;
const pending: ProviderBindingHealPayload[] = [];

function dispatch(payload: ProviderBindingHealPayload): void {
  if (consumer) {
    consumer(payload);
    return;
  }
  pending.push(payload);
}

function isHealPayload(value: unknown): value is ProviderBindingHealPayload {
  if (!value || typeof value !== 'object') return false;
  const kind = (value as ProviderBindingHealPayload).kind;
  const agent = (value as ProviderBindingHealPayload).agent;
  return (kind === 'healed' || kind === 'conflict') && typeof agent === 'string' && agent.length > 0;
}

/**
 * Start listening before React mounts so boot `list_providers` toasts are not dropped.
 * Idempotent. No-op / fail-closed outside Tauri.
 */
export async function startProviderBindingHealListen(): Promise<void> {
  if (!isTauriApp()) {
    throw unavailableError(
      '当前登录纠正提示',
      '当前不是 Tauri 桌面运行时；请使用桌面应用，或开发时注入 mock backend',
    );
  }
  if (listenStarted && listenPromise) {
    await listenPromise;
    return;
  }
  listenStarted = true;
  try {
    listenPromise = (async () => {
      const { listen } = await import('@tauri-apps/api/event');
      return listen<ProviderBindingHealPayload>(PROVIDER_BINDING_HEAL_EVENT, (event) => {
        if (isHealPayload(event.payload)) {
          dispatch(event.payload);
        }
      });
    })();
    await listenPromise;
  } catch (error) {
    listenStarted = false;
    listenPromise = null;
    throw unavailableError(
      '当前登录纠正提示',
      error instanceof Error ? error.message : String(error),
    );
  }
}

/**
 * Subscribe to live-vs-pool login heal/conflict notices.
 * Returns an unsubscribe function.
 */
export async function onProviderBindingHeal(
  handler: (payload: ProviderBindingHealPayload) => void,
): Promise<() => void> {
  if (!isTauriApp()) {
    throw unavailableError(
      '当前登录纠正提示',
      '当前不是 Tauri 桌面运行时；请使用桌面应用，或开发时注入 mock backend',
    );
  }
  consumer = handler;
  const queued = pending.splice(0);
  for (const payload of queued) {
    handler(payload);
  }
  try {
    await startProviderBindingHealListen();
  } catch (error) {
    if (consumer === handler) consumer = null;
    throw error;
  }
  return () => {
    if (consumer === handler) consumer = null;
  };
}
