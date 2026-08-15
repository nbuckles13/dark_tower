import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { resolve } from 'node:path';
import { createRequire } from 'node:module';
import { loadCertFingerprints } from './vite/fingerprints.js';

// R-30: web-app dev/build config.
//
// Build-time SDK version (mirrors sdk-core): the `__SDK_VERSION__` literal the
// aliased sdk-core source resolves. Read from this package's package.json — a
// second read alongside sdk-core's own inline read (sdk-svelte externalizes
// sdk-core and needs none), which stays under the extract-on-third-use line, so
// no shared helper is introduced (@dry-reviewer watch-item B).
const pkgVersion: string = (createRequire(import.meta.url)('./package.json') as { version: string })
  .version;

// AC/GC dev proxy upstream targets — plain HTTP (AC/GC bind plain TcpListeners in
// dev-Kind; @operations). Env-overridable; documented defaults host 8443/8444 on
// loopback. `https` is reserved for the prod bundle only. MC/MH WebTransport is
// NOT proxied (direct https/QUIC with serverCertificateHashes).
const AC_PROXY_TARGET = process.env['VITE_AC_PROXY_TARGET'] ?? 'http://127.0.0.1:8443';
const GC_PROXY_TARGET = process.env['VITE_GC_PROXY_TARGET'] ?? 'http://127.0.0.1:8444';

// Dev cert fingerprints for MC/MH WebTransport pinning (graceful if absent).
const CERT_HASHES = loadCertFingerprints(
  resolve(__dirname, '../../infra/docker/certs/fingerprints.json'),
);

export default defineConfig(({ mode }) => ({
  define: {
    // R-14: dev-only cert-trust path gate; `false` in prod → tree-shaken out.
    __DEV_TRUST_FINGERPRINT__: JSON.stringify(mode !== 'production'),
    // R-29: E2E hooks (window.__darktower_test__ + replay buffer) gate — a
    // SEPARATE flag; `false` in prod so the entire bus module is DCE'd.
    __E2E_HOOKS__: JSON.stringify(mode !== 'production'),
    __SDK_VERSION__: JSON.stringify(pkgVersion),
    // Dev MC/MH cert SHA-256 fingerprints (base64). Empty in prod / when absent.
    __DEV_CERT_SHA256_HASHES__: JSON.stringify(mode !== 'production' ? CERT_HASHES : []),
  },
  resolve: {
    alias: {
      // Source-resolve the workspace SDK packages so `pnpm dev` needs no prebuild
      // and OTel (externalized by sdk-core's own lib build) resolves from
      // sdk-core's node_modules. sdk-svelte's `.svelte.ts` runes compile via the
      // svelte plugin below.
      '@darktower/sdk-core': resolve(__dirname, '../sdk-core/src/index.ts'),
      '@darktower/sdk-svelte': resolve(__dirname, '../sdk-svelte/src/index.ts'),
    },
  },
  plugins: [svelte()],
  server: {
    // AC needs the subdomain in the Host header (ADR-0020 org extraction), so the
    // auth proxy PRESERVES Host (changeOrigin:false) and forwards WHATEVER `*.localhost`
    // label the page was served on — this proxy is subdomain-AGNOSTIC and needs no change
    // per organization. GC resolves org from the JWT, so its proxy may rewrite the host.
    //
    // `demo` is NOT a load-bearing label here any more (R-7, story task #3). It remains the
    // interactive `pnpm dev` default, prefilled in SignUp.svelte/SignIn.svelte and seeded by
    // infra/kind/scripts/setup.sh:seed_demo_org — serve the app at
    // http://demo.localhost:5173 for that. The browser E2E suite runs against a PER-RUN
    // organization instead: scripts/layer7.sh Phase 1h provisions one and exports
    // E2E_ORG_SUBDOMAIN, from which packages/web-app/e2e/env.ts DERIVES the page origin
    // (`http://${orgSubdomain}.localhost:5173`). Hardcoding `demo` anywhere on that path
    // re-creates the cross-run meeting-cap exhaustion the per-run org removes.
    proxy: {
      '/api/v1/auth': { target: AC_PROXY_TARGET, changeOrigin: false, secure: false },
      '/api/v1/meetings': { target: GC_PROXY_TARGET, changeOrigin: true, secure: false },
      '/api/v1/telemetry': { target: GC_PROXY_TARGET, changeOrigin: true, secure: false },
    },
  },
  build: {
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: true,
    rollupOptions: {
      output: {
        // R-14 discipline (mirrors sdk-core): drop inlined source text from the
        // PROD sourcemap so the dev-only strings (`__darktower_test__`,
        // `serverCertificateHashes`, …) cannot re-enter via `sourcesContent`.
        // The prod bundle-content test scans all of `dist/` for their absence.
        sourcemapExcludeSources: mode === 'production',
      },
    },
  },
}));
