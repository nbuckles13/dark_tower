/// <reference types="vite/client" />

// Demo-specific Vite env vars (all optional; defaults live in lib/config.ts).
interface ImportMetaEnv {
  readonly VITE_AC_ORIGIN_TEMPLATE?: string;
  readonly VITE_GC_BASE_URL?: string;
  readonly VITE_TELEMETRY_ENDPOINT?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
