import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

describe('dev-runtime.json', () => {
  it('is the source for Vite, Tauri devUrl, and run.ps1 port', () => {
    const runtime = JSON.parse(
      readFileSync(path.join(root, 'scripts/dev-runtime.json'), 'utf8'),
    ) as { host: string; port: number; e2ePort: number };
    expect(runtime.host).toBe('127.0.0.1');
    expect(runtime.port).toBe(5173);
    expect(runtime.e2ePort).toBe(5174);
    expect(runtime.e2ePort).not.toBe(runtime.port);

    const tauri = readFileSync(path.join(root, 'src-tauri/tauri.conf.json'), 'utf8');
    const origin = `http://${runtime.host}:${runtime.port}`;
    expect(tauri).toContain(`"devUrl": "${origin}"`);
    // Production CSP must not list the Vite HMR origin. During `tauri:dev`
    // the page loads from `devUrl`, so connect-src 'self' already covers it.
    expect(tauri).not.toContain(`ws://${runtime.host}:${runtime.port}`);
    expect(tauri).not.toContain(`ws://localhost:${runtime.port}`);
  });
});
