<script lang="ts">
  // F-SEC-1: a union ALIAS has no fields of its own, so without fixpoint closure this
  // retains a password-carrying object and produces NO finding — using an ENUMERATED
  // retention idiom, not a novel one. Split across files so the closure is exercised
  // THROUGH the cross-file path rather than beside it.
  import type { JoinCredentials } from './pos_union_decl.js';
  let saved = $state<JoinCredentials | undefined>(undefined);
</script>
