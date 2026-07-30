<script lang="ts">
  // Retention site INTACT — only the credential field differs from the positive pair.
  // If the negative also dropped the retention site, the pair would pass for the wrong
  // reason: a tautological fixture one level subtler than the double-exemption bug.
  import type { AuthSession } from './neg_xfile_decl.js';
  let auth = $state<AuthSession | undefined>(undefined);
</script>
