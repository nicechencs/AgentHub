import { defineConfig } from 'vitest/config';
import path from 'node:path';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const rootDir = path.dirname(fileURLToPath(import.meta.url));
const packageVersion = JSON.parse(
  readFileSync(path.resolve(rootDir, 'package.json'), 'utf8'),
).version as string;

export default defineConfig({
  resolve: {
    alias: {
      '@': path.resolve(rootDir, 'src'),
      // Tests always use mock backend (instantiate / façade under mock).
      '#backend': path.resolve(rootDir, 'src/dev/mocks/create-backend.ts'),
      '#oauth-flow-dialog': path.resolve(rootDir, 'src/dev/mocks/OAuthFlowDialog.tsx'),
    },
  },
  define: {
    'import.meta.env.VITE_APP_VERSION': JSON.stringify(packageVersion),
    'import.meta.env.VITE_BACKEND': JSON.stringify('mock'),
  },
  test: {
    environment: 'node',
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    setupFiles: ['src/test/setup.ts'],
  },
});
