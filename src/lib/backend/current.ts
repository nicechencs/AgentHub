/**
 * 默认生产实现入口。
 * 实际装配由 Vite alias `#backend` 决定：
 * - pnpm dev / build → tauri/create-backend
 * - pnpm dev:mock / vitest → dev/mocks/create-backend
 */
export { createBackend } from '#backend';
