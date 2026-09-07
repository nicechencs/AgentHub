/**
 * Projects → Chat 跳转时的一次性 bootstrap（sessionStorage）。
 * 避免把长 prompt 塞进 URL。
 */
import type { ChatBootstrap } from '@/lib/types';
import { readStorageItem, removeStorageItem, StorageKey } from '@/lib/storage-key';

const KEY = StorageKey.chatBootstrap;
let bootstrapGeneration = 0;

export function isChatBootstrapHandoff(from: string | null | undefined): boolean {
  return from === 'projects' || from === 'shell';
}

export function chatBootstrapGeneration(): number {
  return bootstrapGeneration;
}

export function sameChatBootstrap(a: ChatBootstrap, b: ChatBootstrap): boolean {
  return (
    (a.cwd ?? '') === (b.cwd ?? '') &&
    (a.title ?? '') === (b.title ?? '') &&
    (a.prompt ?? '') === (b.prompt ?? '') &&
    a.agentIds.length === b.agentIds.length &&
    a.agentIds.every((id, index) => id === b.agentIds[index])
  );
}

export function setChatBootstrap(payload: ChatBootstrap): boolean {
  try {
    sessionStorage.setItem(KEY, JSON.stringify(payload));
    bootstrapGeneration += 1;
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

function parseChatBootstrap(raw: string): ChatBootstrap | null {
  const data = JSON.parse(raw) as ChatBootstrap;
  if (!data) return null;
  const agentIds = Array.isArray(data.agentIds) ? data.agentIds.filter(Boolean) : [];
  const cwd = typeof data.cwd === 'string' ? data.cwd.trim() : '';
  if (agentIds.length === 0 && !cwd) return null;
  return { ...data, agentIds, cwd: cwd || data.cwd };
}

/** 读取并清除，保证只消费一次 */
export function takeChatBootstrap(): ChatBootstrap | null {
  try {
    const raw = readStorageItem(sessionStorage, KEY);
    if (raw == null) return null;
    removeStorageItem(sessionStorage, KEY);
    return parseChatBootstrap(raw);
  } catch {
    try {
      removeStorageItem(sessionStorage, KEY);
    } catch {
      /* ignore */
    }
    return null;
  }
}

/** Write back only when no newer handoff replaced this payload. */
export function restoreChatBootstrapIfUnchanged(
  taken: ChatBootstrap,
  generation: number,
): boolean {
  if (generation !== bootstrapGeneration) return false;
  try {
    const raw = readStorageItem(sessionStorage, KEY);
    if (raw != null) {
      try {
        const current = parseChatBootstrap(raw);
        if (current && !sameChatBootstrap(current, taken)) return false;
      } catch {
        return false;
      }
    }
    return setChatBootstrap(taken);
  } catch {
    return false;
  }
}
