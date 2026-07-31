import { defineConfig } from 'vite';
import dts from 'vite-plugin-dts';
import { resolve } from 'node:path';
import { createRequire } from 'node:module';

// Build-time SDK version (task #12, R-24/R-26): read from package.json so the
// OTel resource `service.version` + the `client_version` log/metric field track
// the published version with no hand-maintained duplicate.
const pkgVersion: string = (createRequire(import.meta.url)('./package.json') as { version: string })
  .version;

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
    __SDK_VERSION__: JSON.stringify(pkgVersion),
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
      // Task #12 (Gate-1, approved): the `@opentelemetry/*` packages are
      // EXTERNALIZED, not bundled. The browser SDK ships bare
      // `import '@opentelemetry/...'` and the bundling consumer (web-app)
      // resolves them as runtime deps. This (a) keeps sdk-core's `dist` small,
      // (b) guarantees ZERO OTel transitive string literals land in `dist`, so
      // the R-14 dist-wide forbidden-token scan in `tests/bundle-content.test.ts`
      // is unaffected, and (c) lets the unit tier mock OTel via the
      // constructor-injected `Meter` rather than running the real SDK.
      external: [/^@opentelemetry\//],
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
      outDirs: 'dist',
      tsconfigPath: './tsconfig.build.json',
      include: ['src/**/*'],
      exclude: ['src/**/__tests__/**', 'src/**/*.test.ts'],
    }),
  ],
}));
