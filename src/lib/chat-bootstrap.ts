/**
 * Projects → Chat 跳转时的一次性 bootstrap（sessionStorage）。
 * 避免把长 prompt 塞进 URL。
 */
import type { ChatBootstrap } from '@/lib/types';
import { readStorageItem, removeStorageItem, StorageKey } from '@/lib/storage-key';

const KEY = StorageKey.chatBootstrap;

export function setChatBootstrap(payload: ChatBootstrap): boolean {
  try {
    sessionStorage.setItem(KEY, JSON.stringify(payload));
    return true;
  } catch {
    return false;
  }
}

const BOOTSTRAP_FIT_LIMITS = [200_000, 80_000, 24_000, 4_000] as const;

/** Write bootstrap; if quota fails, shrink the prompt until it fits. */
export function setChatBootstrapFitting(
  payload: ChatBootstrap,
  shrinkPrompt: (limit: number) => string,
): boolean {
  if (setChatBootstrap(payload)) return true;
  for (const limit of BOOTSTRAP_FIT_LIMITS) {
    if (setChatBootstrap({ ...payload, prompt: shrinkPrompt(limit) })) return true;
  }
  return false;
}

/** 读取并清除，保证只消费一次 */
export function takeChatBootstrap(): ChatBootstrap | null {
  try {
    const raw = readStorageItem(sessionStorage, KEY);
    if (raw == null) return null;
    removeStorageItem(sessionStorage, KEY);
    const data = JSON.parse(raw) as ChatBootstrap;
    if (!data || !Array.isArray(data.agentIds) || data.agentIds.length === 0) return null;
    return data;
  } catch {
    try {
      removeStorageItem(sessionStorage, KEY);
    } catch {
      /* ignore */
    }
    return null;
  }
}
