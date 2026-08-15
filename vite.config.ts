import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'node:path';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { buildBootCriticalCss, buildDesignTokensCss } from './src/styles/tokens';

const rootDir = path.dirname(fileURLToPath(import.meta.url));
const packageVersion = JSON.parse(
  readFileSync(path.resolve(rootDir, 'package.json'), 'utf8'),
).version as string;
const tauriBackend = path.resolve(rootDir, 'src/lib/backend/tauri/create-backend.ts');
const mockBackend = path.resolve(rootDir, 'src/dev/mocks/create-backend.ts');
const productionOAuthDialog = path.resolve(
  rootDir,
  'src/components/connect/OAuthFlowDialog.tsx',
);
const mockOAuthDialog = path.resolve(rootDir, 'src/dev/mocks/OAuthFlowDialog.tsx');

const DESIGN_TOKENS_VIRTUAL = 'virtual:agenthub-design-tokens.css';
const DESIGN_TOKENS_RESOLVED = '\0' + DESIGN_TOKENS_VIRTUAL;
const BOOT_TOKENS_RE =
  /\/\* @@design-tokens-boot:start@@ \*\/[\s\S]*?\/\* @@design-tokens-boot:end@@ \*\//;

/**
 * Single source of truth: `src/styles/tokens.ts`.
 * - Virtual CSS for the app bundle
 * - Injects boot-critical vars into index.html so splash paints before JS
 */
function designTokensPlugin(): Plugin {
  return {
    name: 'agenthub-design-tokens',
    resolveId(id) {
      if (id === DESIGN_TOKENS_VIRTUAL) return DESIGN_TOKENS_RESOLVED;
    },
    load(id) {
      if (id === DESIGN_TOKENS_RESOLVED) return buildDesignTokensCss();
    },
    transformIndexHtml(html) {
      const boot = buildBootCriticalCss();
      if (!BOOT_TOKENS_RE.test(html)) {
        this.warn(
          'index.html missing /* @@design-tokens-boot:start@@ */ markers; boot colors may drift',
        );
        return html;
      }
      return html.replace(
        BOOT_TOKENS_RE,
        `/* @@design-tokens-boot:start@@ */\n${boot}\n      /* @@design-tokens-boot:end@@ */`,
      );
    },
    handleHotUpdate({ file, server }) {
      if (!file.replace(/\\/g, '/').endsWith('/src/styles/tokens.ts')) return;
      const mod = server.moduleGraph.getModuleById(DESIGN_TOKENS_RESOLVED);
      if (mod) server.moduleGraph.invalidateModule(mod);
      server.ws.send({ type: 'full-reload' });
      return [];
    },
  };
}

/** Fail production builds if forbidden modules enter the module graph. */
function productionModuleGraphGuard(): Plugin {
  const forbidden = [
    /[\\/]src[\\/]dev[\\/]/,
    /[\\/]src[\\/]test[\\/]/,
    /\.test\.[cm]?[jt]sx?$/,
    /\.spec\.[cm]?[jt]sx?$/,
  ];

  return {
    name: 'agenthub-production-module-graph-guard',
    apply: 'build',
    generateBundle(_options, bundle) {
      const hits: string[] = [];
      for (const chunk of Object.values(bundle)) {
        if (chunk.type !== 'chunk') continue;
        for (const id of Object.keys(chunk.modules ?? {})) {
          const normalized = id.replace(/\0/g, '');
          if (forbidden.some((re) => re.test(normalized))) {
            hits.push(normalized);
          }
        }
      }
      if (hits.length) {
        const unique = [...new Set(hits)].sort();
        throw new Error(
          [
            'Production build includes forbidden modules (dev mock / test):',
            ...unique.map((h) => `  - ${h}`),
            'Mock must only be selected via pnpm dev:mock / vitest, never pnpm build.',
          ].join('\n'),
        );
      }
    },
  };
}

export default defineConfig(({ mode, command }) => {
  // pnpm dev:mock → mode mock + serve
  // pnpm dev / tauri dev / pnpm build → always Tauri adapter
  const useMock = command === 'serve' && mode === 'mock';
  const backendEntry = useMock ? mockBackend : tauriBackend;
  const oauthDialogEntry = useMock ? mockOAuthDialog : productionOAuthDialog;

  return {
    plugins: [designTokensPlugin(), react(), productionModuleGraphGuard()],
    // Tauri 生产包用相对路径加载 assets，避免白屏
    base: './',
    clearScreen: false,
    resolve: {
      alias: {
        '@': path.resolve(rootDir, 'src'),
        '#backend': backendEntry,
        // OAuth 演示对话框仅 mock 模式；生产只解析 unavailable 实现
        '#oauth-flow-dialog': oauthDialogEntry,
      },
    },
    server: {
      host: '127.0.0.1',
      port: 5173,
      strictPort: true,
      // Windows: cargo locks target/*.exe during compile; Vite watching them throws EBUSY.
      watch: {
        ignored: ['**/src-tauri/**', '**/target/**', '**/crates/**/target/**'],
      },
    },
    envPrefix: ['VITE_', 'TAURI_'],
    define: {
      'import.meta.env.VITE_BACKEND': JSON.stringify(useMock ? 'mock' : 'tauri'),
      // From package.json only — never hand-edit display version strings in src/.
      'import.meta.env.VITE_APP_VERSION': JSON.stringify(packageVersion),
    },
    build: {
      target: 'esnext',
      minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
      sourcemap: !!process.env.TAURI_DEBUG,
    },
  };
});
