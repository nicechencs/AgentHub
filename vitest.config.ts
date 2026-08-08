import { defineConfig } from 'vitest/config';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const rootDir = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  resolve: {
    alias: {
      '@': path.resolve(rootDir, 'src'),
      // Tests always use mock backend (instantiate / façade under mock).
      '#backend': path.resolve(rootDir, 'src/dev/mocks/create-backend.ts'),
      '#oauth-flow-dialog': path.resolve(rootDir, 'src/dev/mocks/OAuthFlowDialog.tsx'),
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
    setupFiles: ['src/test/setup.ts'],
  },
});
