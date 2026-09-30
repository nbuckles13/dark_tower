/// <reference types="vite/client" />

// Demo-specific Vite env vars (all optional; defaults live in lib/config.ts).
interface ImportMetaEnv {
  readonly VITE_AC_ORIGIN_TEMPLATE?: string;
  readonly VITE_GC_BASE_URL?: string;
  readonly VITE_TELEMETRY_ENDPOINT?: string;
  /**
   * N — the audio receive-slot count this client declares (story 2 R-1). Parsed
   * strictly by the SDK's `parseReceiveSlots`; see `src/lib/config.ts`.
   */
  readonly VITE_DT_RECEIVE_SLOTS?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
