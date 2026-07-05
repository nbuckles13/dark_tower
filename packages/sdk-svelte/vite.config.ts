import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import dts from 'vite-plugin-dts';
import { resolve } from 'node:path';

// R-10 / ADR-0028 §2: Vite library mode → ESM primary + CJS fallback + `.d.ts`.
// `@darktower/sdk-core` and `svelte` are EXTERNALIZED (peer/workspace deps the
// consuming app resolves) so this package's `dist` carries only the adapter
// glue — no sdk-core or svelte runtime is bundled in, and no sdk-core build-time
// `define` literal can land here.
export default defineConfig({
  build: {
    lib: {
      entry: { index: resolve(__dirname, 'src/index.ts') },
      formats: ['es', 'cjs'],
      fileName: (format, entryName) => {
        const ext = format === 'es' ? 'mjs' : 'cjs';
        return `${entryName}.${ext}`;
      },
    },
    rollupOptions: {
      external: [/^@darktower\/sdk-core/, 'svelte', /^svelte\//],
    },
    sourcemap: true,
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
  },
  plugins: [
    svelte(),
    dts({
      entryRoot: 'src',
      outDir: 'dist',
      tsconfigPath: './tsconfig.build.json',
      include: ['src/**/*'],
      exclude: ['src/**/__tests__/**', 'src/**/*.test.ts'],
    }),
  ],
});
