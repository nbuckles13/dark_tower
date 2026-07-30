<script lang="ts">
  // LENS-ONLY. Isolates sub-check (ii) from (i): the password lives in a bare local,
  // is posted while a token is held, and is stored NOWHERE. Zero retention.
  //
  // Without this fixture the only (ii) case is the real pre-fix shape, which satisfies
  // BOTH sub-checks — so (ii) would read as covered while being tested only through
  // (i). Mechanical guard silent by construction; the assertion is the Gate-3 report.
  let { userToken }: { userToken: string } = $props();
  let password = $state('');

  async function submit(): Promise<void> {
    await fetch('/api/v1/elevate', {
      method: 'POST',
      headers: { authorization: `Bearer ${userToken}` },
      body: JSON.stringify({ password }),
    });
  }
</script>

<input type="password" bind:value={password} />
<button onclick={submit}>Submit</button>
