/**
 * Projects → Chat 跳转时的一次性 bootstrap（sessionStorage）。
 * 避免把长 prompt 塞进 URL。
 */
import type { ChatBootstrap } from '@/lib/types';

const KEY = 'agenthub.chat.bootstrap';

export function setChatBootstrap(payload: ChatBootstrap): void {
  try {
    sessionStorage.setItem(KEY, JSON.stringify(payload));
  } catch {
    // quota / private mode — ignore; caller can still navigate
  }
}

/** 读取并清除，保证只消费一次 */
export function takeChatBootstrap(): ChatBootstrap | null {
  try {
    const raw = sessionStorage.getItem(KEY);
    if (!raw) return null;
    sessionStorage.removeItem(KEY);
    const data = JSON.parse(raw) as ChatBootstrap;
    if (!data || !Array.isArray(data.agentIds) || data.agentIds.length === 0) return null;
    return data;
  } catch {
    try {
      sessionStorage.removeItem(KEY);
    } catch {
      /* ignore */
    }
    return null;
  }
}
