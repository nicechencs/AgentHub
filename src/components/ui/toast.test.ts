import { describe, expect, it, vi } from 'vitest';
import { formatToastClipboardText, DEFAULT_TOAST_DURATION_MS } from './toast';
import { ToastAutoDismissController } from './toast-auto-dismiss';

describe('formatToastClipboardText', () => {
  it('returns title only when description is empty', () => {
    expect(formatToastClipboardText('切换失败')).toBe('切换失败');
    expect(formatToastClipboardText('切换失败', '  ')).toBe('切换失败');
  });

  it('joins title and description with a newline', () => {
    expect(formatToastClipboardText('切换失败', 'token expired')).toBe('切换失败\ntoken expired');
  });

  it('trims surrounding whitespace', () => {
    expect(formatToastClipboardText('  a  ', '  b  ')).toBe('a\nb');
  });
});

describe('ToastAutoDismissController', () => {
  function createHarness(durationMs: number) {
    let now = 0;
    const timers = new Map<number, { fireAt: number; fn: () => void }>();
    let nextId = 1;
    const onDismiss = vi.fn();

    const controller = new ToastAutoDismissController({
      durationMs,
      onDismiss,
      now: () => now,
      setTimer: (fn, ms) => {
        const id = nextId++;
        timers.set(id, { fireAt: now + ms, fn });
        return id;
      },
      clearTimer: (id) => {
        timers.delete(id);
      },
    });

    const advance = (ms: number) => {
      now += ms;
      for (const [id, t] of [...timers.entries()]) {
        if (t.fireAt <= now) {
          timers.delete(id);
          t.fn();
        }
      }
    };

    return { controller, onDismiss, advance, timerCount: () => timers.size };
  }

  it('dismisses after the configured duration', () => {
    const { controller, onDismiss, advance } = createHarness(1_000);
    controller.start();
    advance(999);
    expect(onDismiss).not.toHaveBeenCalled();
    advance(1);
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it('pauses on pause() and resumes remaining time', () => {
    const { controller, onDismiss, advance, timerCount } = createHarness(1_000);
    controller.start();
    advance(400);
    controller.pause();
    expect(timerCount()).toBe(0);
    advance(5_000);
    expect(onDismiss).not.toHaveBeenCalled();

    controller.resume();
    advance(599);
    expect(onDismiss).not.toHaveBeenCalled();
    advance(1);
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it('ignores non-finite duration and dispose cancels pending timer', () => {
    const infinite = createHarness(Number.POSITIVE_INFINITY);
    infinite.controller.start();
    infinite.advance(DEFAULT_TOAST_DURATION_MS * 2);
    expect(infinite.onDismiss).not.toHaveBeenCalled();

    const finite = createHarness(2_000);
    finite.controller.start();
    finite.controller.dispose();
    finite.advance(3_000);
    expect(finite.onDismiss).not.toHaveBeenCalled();
  });
});
