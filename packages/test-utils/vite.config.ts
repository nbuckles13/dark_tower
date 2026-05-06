import { defineConfig } from 'vite';
import dts from 'vite-plugin-dts';
import { resolve } from 'node:path';

export default defineConfig({
  build: {
    lib: {
      entry: {
        index: resolve(__dirname, 'src/index.ts'),
        'test-only/signer': resolve(__dirname, 'src/test-only/signer.ts'),
      },
      formats: ['es', 'cjs'],
      fileName: (format, entryName) => {
        const ext = format === 'es' ? 'mjs' : 'cjs';
        return `${entryName}.${ext}`;
      },
    },
    rollupOptions: {
      external: ['@noble/ed25519', 'node:crypto'],
    },
    sourcemap: true,
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
  },
  plugins: [
    dts({
      entryRoot: 'src',
      outDir: 'dist',
      tsconfigPath: './tsconfig.build.json',
      include: ['src/**/*'],
      exclude: ['src/**/__tests__/**', 'src/**/*.test.ts'],
    }),
  ],
});
