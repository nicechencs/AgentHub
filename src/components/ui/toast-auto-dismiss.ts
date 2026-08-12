/**
 * Toast 自动关闭计时（不绑 React）。
 *
 * 不用 Radix 内置 duration：它在 focusin / window blur 时 pause，桌面端选文字、
 * 点复制、切窗后经常再也 resume 不了，表现为「通知不消失」。
 * 本实现仅在显式 pause/resume（UI 映射为 hover）时挂起。
 */
export type ToastAutoDismissOptions = {
  durationMs: number;
  onDismiss: () => void;
  /** 可注入，便于测试 */
  now?: () => number;
  setTimer?: (fn: () => void, ms: number) => number;
  clearTimer?: (id: number) => void;
};

export class ToastAutoDismissController {
  private remainingMs: number;
  private startedAt: number | null = null;
  private timerId: number | null = null;
  private paused = false;
  private disposed = false;
  private readonly onDismiss: () => void;
  private readonly now: () => number;
  private readonly setTimer: (fn: () => void, ms: number) => number;
  private readonly clearTimerFn: (id: number) => void;

  constructor(opts: ToastAutoDismissOptions) {
    this.remainingMs = opts.durationMs;
    this.onDismiss = opts.onDismiss;
    this.now = opts.now ?? (() => performance.now());
    this.setTimer =
      opts.setTimer ?? ((fn, ms) => window.setTimeout(fn, ms) as unknown as number);
    this.clearTimerFn =
      opts.clearTimer ?? ((id) => window.clearTimeout(id as unknown as number));
  }

  /** 开始（或按当前 remaining 重新武装） */
  start(): void {
    if (this.disposed || this.paused) return;
    this.clearTimer();
    if (!Number.isFinite(this.remainingMs)) return;
    if (this.remainingMs <= 0) {
      this.onDismiss();
      return;
    }
    this.startedAt = this.now();
    this.timerId = this.setTimer(() => {
      this.timerId = null;
      this.startedAt = null;
      if (!this.disposed) this.onDismiss();
    }, this.remainingMs);
  }

  pause(): void {
    if (this.disposed || this.paused) return;
    this.paused = true;
    if (this.startedAt != null) {
      const elapsed = this.now() - this.startedAt;
      this.remainingMs = Math.max(0, this.remainingMs - elapsed);
      this.startedAt = null;
    }
    this.clearTimer();
  }

  resume(): void {
    if (this.disposed || !this.paused) return;
    this.paused = false;
    this.start();
  }

  dispose(): void {
    this.disposed = true;
    this.clearTimer();
    this.startedAt = null;
  }

  /** @internal tests */
  getRemainingMsForTest(): number {
    return this.remainingMs;
  }

  private clearTimer(): void {
    if (this.timerId != null) {
      this.clearTimerFn(this.timerId);
      this.timerId = null;
    }
  }
}
