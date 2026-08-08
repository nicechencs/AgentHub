/**
 * 轻量前端 logger：dev 打 console，带时间戳与 scope。
 */

type LogLevel = 'debug' | 'info' | 'warn' | 'error';

const isDev = (() => {
  try {
    // Vite injects import.meta.env; guard for non-Vite typecheck contexts.
    const env = (import.meta as ImportMeta & { env?: { DEV?: boolean } }).env;
    return Boolean(env?.DEV);
  } catch {
    return false;
  }
})();

function timestamp(): string {
  return new Date().toISOString();
}

function formatPrefix(level: LogLevel, scope?: string): string {
  const scopePart = scope ? ` [${scope}]` : '';
  return `${timestamp()} ${level.toUpperCase()}${scopePart}`;
}

function emit(level: LogLevel, scope: string | undefined, args: unknown[]): void {
  if (!isDev) return;
  const prefix = formatPrefix(level, scope);
  const fn =
    level === 'debug'
      ? console.debug
      : level === 'info'
        ? console.info
        : level === 'warn'
          ? console.warn
          : console.error;
  fn(prefix, ...args);
}

function createLogger(scope?: string) {
  return {
    debug: (...args: unknown[]) => emit('debug', scope, args),
    info: (...args: unknown[]) => emit('info', scope, args),
    warn: (...args: unknown[]) => emit('warn', scope, args),
    error: (...args: unknown[]) => emit('error', scope, args),
    /** 派生带 scope 的子 logger */
    scope: (name: string) => createLogger(scope ? `${scope}:${name}` : name),
  };
}

/** 默认全局 logger（可 `.scope('api')` 细分） */
export const logger = createLogger();
