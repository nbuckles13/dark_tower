// File: packages/sdk-core/src/media/teardown/teardown.ts
//
// Deterministic, idempotent media teardown.
//
// ---------------------------------------------------------------------------
// REGISTER AT ACQUISITION, BEFORE THE NEXT AWAIT
// ---------------------------------------------------------------------------
//
// The property that matters is not "teardown releases everything" — it is
// "teardown reaches a PARTIALLY-CONSTRUCTED pipeline". Setup is a chain of
// awaits: `getUserMedia` resolves, then the encoder is configured, then the
// decoder, then the playback context. If `AudioEncoder.configure` throws, the
// microphone track already exists, and a leaked track keeps the microphone hot
// and the browser's recording indicator lit after the meeting ends. That is a
// user-visible privacy failure, not a memory nit.
//
// So each resource is registered HERE the moment it exists, before the next
// await — the pattern `MediaTransport.#connectOne` already uses for transports.
//
// ---------------------------------------------------------------------------
// ONE FAILING DISPOSER MUST NOT STRAND THE REST
// ---------------------------------------------------------------------------
//
// Disposers run in reverse registration order (last acquired, first released)
// and each is isolated: a throw is captured and the loop continues. The captured
// errors are returned rather than swallowed, so a caller can surface them —
// silence here would be the masked failure CLAUDE.md's fail-loudly rule names.
//
// ---------------------------------------------------------------------------
// WHAT THIS PATH IS OBLIGED TO CLEAR
// ---------------------------------------------------------------------------
//
// ADR-0028 §5 requires explicit cleanup on disconnect/logout, and the codec
// layer left the wiring as this task's obligation:
//
//   * `TransmitKeyCache.clear()` and `ReplayWindow.clear()` — the receive-side
//     state, whose `clear()` overwrites key buffers before dropping references.
//   * `TransmitKeyManager.clear()` — the send-side keys, same treatment.
//   * the meeting KEK, zeroed and dropped through the KEK-source seam.
//   * the identity signing key, dropped (non-extractable, so there are no bytes
//     to overwrite — the platform holds them).
//   * capture tracks, encoder, decoder, playback context, the datagram reader,
//     and the rotation timer.
//
// NOTE ON LOGGING: this directory is a SIBLING of the hot path, so ADR-0036
// §11's directory-scoped deny does not reach it and logging here is permitted by
// design. It is nevertheless absent, deliberately: "log what we cleaned up" is
// the single most natural line to add to a zeroisation routine, and it is the
// one place where naming every key at once is idiomatic.

/** A named disposer. The name is a static string, never derived from state. */
interface Registration {
  readonly name: string;
  readonly dispose: () => void | Promise<void>;
}

/**
 * Ordered, idempotent disposer registry.
 *
 * Safe to call `dispose()` during setup, from an error path, and more than once.
 */
export class TeardownRegistry {
  readonly #registrations: Registration[] = [];
  #disposed = false;

  /**
   * Register a disposer.
   *
   * Call this the moment the resource exists, BEFORE the next await. Registering
   * after the awaits complete is the bug this class exists to prevent, and it is
   * invisible on the happy path.
   *
   * If teardown has already run, the disposer is invoked immediately: a resource
   * that finished acquiring after `dispose()` was called must not survive it.
   * This is the race an async setup cancelled mid-flight actually produces.
   */
  register(name: string, dispose: () => void | Promise<void>): void {
    if (this.#disposed) {
      void Promise.resolve()
        .then(dispose)
        .catch(() => {
          // Nothing to report to: the caller has already torn down and is not
          // awaiting this. Isolating it here is what keeps a late disposer from
          // becoming an unhandled rejection in an embedder's page.
        });
      return;
    }
    this.#registrations.push({ name, dispose });
  }

  /** Whether `dispose()` has run. */
  get isDisposed(): boolean {
    return this.#disposed;
  }

  /**
   * Run every disposer in reverse registration order.
   *
   * Idempotent: a second call is a no-op.
   *
   * @returns the disposers that threw, by NAME and error. Returned rather than
   * thrown so one bad disposer cannot prevent the caller from completing its own
   * teardown, and returned rather than dropped so the failure is not masked.
   */
  async dispose(): Promise<readonly { readonly name: string; readonly error: unknown }[]> {
    if (this.#disposed) return [];
    this.#disposed = true;
    const failures: { name: string; error: unknown }[] = [];
    for (let i = this.#registrations.length - 1; i >= 0; i -= 1) {
      const registration = this.#registrations[i];
      if (!registration) continue;
      try {
        await registration.dispose();
      } catch (error) {
        failures.push({ name: registration.name, error });
      }
    }
    this.#registrations.length = 0;
    return failures;
  }
}
