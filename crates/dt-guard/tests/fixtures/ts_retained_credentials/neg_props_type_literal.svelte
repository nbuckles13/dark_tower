<script lang="ts">
  // 3. $props() TYPE LITERAL — real shape: CreateMeeting.svelte:16 / JoinMeeting.svelte:18.
  //    Props are transient, and the annotation is an inline `{`-literal rather than a
  //    bare type name, which is what the binding rule keys on.
  //
  //    This one also mattered for the pre-B2 EVIDENCE run: pre-fix `AuthResult` carried
  //    `password`, so without this exclusion the primary evidence would have shown three
  //    hits and two of them would have needed reconciling as false. The exclusion is
  //    what makes the BEFORE picture clean, not just the after.
  import type { LoginParams } from './neg_transient_login_params.js';
  let { creds }: { creds: LoginParams } = $props();
</script>

<p>{creds.email}</p>
