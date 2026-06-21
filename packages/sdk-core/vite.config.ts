import { defineConfig } from 'vite';
import dts from 'vite-plugin-dts';
import { resolve } from 'node:path';

// R-14: `__DEV_TRUST_FINGERPRINT__` is a build-time literal. In production
// builds it is substituted with `false`, so the dev-only
// `serverCertificateHashes` trust branch in `BrowserWebTransport` becomes
// statically dead and is tree-shaken out of the bundle (verified by
// `tests/bundle-content.test.ts`). In dev/test builds it is `true`, enabling
// the self-signed-cert fingerprint path whose VALUE is supplied at runtime via
// config — never hardcoded here.
export default defineConfig(({ mode }) => ({
  define: {
    __DEV_TRUST_FINGERPRINT__: JSON.stringify(mode !== 'production'),
  },
  build: {
    lib: {
      entry: {
        index: resolve(__dirname, 'src/index.ts'),
      },
      formats: ['es', 'cjs'],
      fileName: (format, entryName) => {
        const ext = format === 'es' ? 'mjs' : 'cjs';
        return `${entryName}.${ext}`;
      },
    },
    rollupOptions: {
      external: [],
      output: {
        // R-14: do NOT embed original source text in production sourcemaps.
        // The dev-trust runtime object key + source comments would otherwise
        // land in `dist/*.map` via `sourcesContent`, which the R-14 dt-guard's
        // dist scan reads. Mappings are preserved (line/col), only the inlined
        // source bodies are dropped — standard for a shipped library map.
        sourcemapExcludeSources: mode === 'production',
      },
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
}));
