/**
 * App version helpers for the frontend shell.
 *
 * Product version is NOT hard-coded here.
 * - Tauri GUI: `@tauri-apps/api/app` `getVersion()` (from tauri.conf / Cargo)
 * - Mock / Vite inject: `import.meta.env.VITE_APP_VERSION` (from package.json)
 * - Rust core / CLI: `env!("CARGO_PKG_VERSION")`
 *
 * Keep release bumps in package.json + Cargo workspace + tauri.conf only.
 */

/** Fallback when the shell cannot report a version (should not show a fake semver). */
export const UNKNOWN_APP_VERSION = 'unknown';

/** package.json version injected by Vite (`vite.config.ts`). */
export function packageAppVersion(): string {
  const v = import.meta.env.VITE_APP_VERSION;
  return typeof v === 'string' && v.trim() ? v.trim() : UNKNOWN_APP_VERSION;
}
