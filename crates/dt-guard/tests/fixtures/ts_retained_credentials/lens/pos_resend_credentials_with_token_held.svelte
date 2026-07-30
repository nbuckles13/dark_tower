<script lang="ts">
  // LENS-ONLY (sub-check ii). The mechanical guard is EXPECTED TO BE SILENT here —
  // the layer is in the path, so silence is never ambiguous between "correctly quiet"
  // and "detector disabled".
  //
  // The pre-fix JoinMeeting.credentials() shape: builds a password-bearing credential
  // for an outbound join while `userToken` is reachable on the very same object.
  // Re-sending a credential is a TRANSMISSION event, not a retention one, so no
  // syntax matcher can reach it.
  import type { AuthResult } from '../pos_xfile_decl.js';
  let { auth }: { auth: AuthResult } = $props();

  async function join(): Promise<void> {
    await fetch('/api/v1/join', {
      method: 'POST',
      body: JSON.stringify({ email: auth.email, password: auth.password }),
    });
  }
</script>

<button onclick={join}>Join</button>
