<script lang="ts">
  // LENS-ONLY, and the FP TRAP. The SignIn.svelte shape: a password bound to the input
  // that collects it, exchanged for a token, never propagated. A lens that flags this
  // is broken — it would flag every auth form anyone ever writes.
  //
  // SG-3/SG-11: the boundary is the CROSSING, not the holding. This file never crosses.
  let email = $state('');
  let password = $state('');
  let token = $state('');

  async function signIn(): Promise<void> {
    const res = await fetch('/api/v1/auth/user/token', {
      method: 'POST',
      body: JSON.stringify({ email, password }),
    });
    token = ((await res.json()) as { accessToken: string }).accessToken;
    password = '';
  }
</script>

<input type="email" bind:value={email} />
<input type="password" bind:value={password} />
<button onclick={signIn}>Sign in</button>
