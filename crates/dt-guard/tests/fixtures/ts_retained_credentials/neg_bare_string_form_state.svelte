<script lang="ts">
  // Documented blind spot, PINNED as not-firing. A bare string has no type annotation
  // for the retention rule to key on, so this is invisible to the mechanical guard —
  // and it is CORRECT code: a credential bound to the input that collects it, for that
  // view's lifetime.
  //
  // The pin exists so a later author does not "fix" the gap and turn the guard into a
  // false-positive generator on every login form. The dangerous half of this idiom —
  // a bare credential that crosses OUT of the collecting view — belongs to the
  // semantic lens, which can read intent.
  let password = $state('');
  let email = $state('');
</script>

<input type="password" bind:value={password} />
<input type="email" bind:value={email} />
