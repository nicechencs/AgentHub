/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_BACKEND: 'mock' | 'tauri';
  /** Injected from root package.json at Vite config load time. */
  readonly VITE_APP_VERSION: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

/** Generated from `src/styles/tokens.ts` by the Vite design-tokens plugin. */
declare module 'virtual:agenthub-design-tokens.css';
