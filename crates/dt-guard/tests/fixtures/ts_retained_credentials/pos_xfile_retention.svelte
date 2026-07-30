<script lang="ts">
  // Cross-file positive, RETENTION half. Mirrors packages/web-app/src/App.svelte
  // pre-fix. Fires ONLY when scanned as a set with pos_xfile_decl.ts, because the
  // declaration that makes AuthResult credential-bearing lives in the other file.
  //
  // This pair is the regression lock for F11: a per-file kernel hands Rule 2 an empty
  // declaration index, so every rule-2 assertion returns silent — and silent is what a
  // negative fixture asserts. The whole matrix would go green with the rule disabled.
  import type { AuthResult } from './pos_xfile_decl.js';
  let auth = $state<AuthResult | undefined>(undefined);
</script>
