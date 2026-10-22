// File: packages/sdk-core/src/media/pipeline/decoderRestartGate.ts
//
// HOT PATH (consulted when a decoder is missing). No logging, no metrics.
//
// THE ONE meaning of `media.ingress.decoderRestartBackoffMs`, for every decoder
// the receive side runs — an audio lane's (`receiveLanes.ts`) and a video
// slot's (`videoSlotPipeline.ts`): after a fault, a replacement decoder may be
// created only once the backoff has fully elapsed on the injected clock
// (`now - lastFault >= backoff`). Without it a sender whose frames fault the
// decoder every time would churn a decoder per frame and turn the decoder-error
// counter into per-frame telemetry. Written once so the `<` / `<=` edge cannot
// drift between the two kinds.

/** See the module header. */
export class DecoderRestartGate {
  readonly #backoffMs: number;
  readonly #clock: () => number;
  #lastFaultAtMs: number | undefined;

  constructor(backoffMs: number, clock: () => number) {
    this.#backoffMs = backoffMs;
    this.#clock = clock;
  }

  /** The decoder faulted now. */
  fault(): void {
    this.#lastFaultAtMs = this.#clock();
  }

  /** Whether a replacement decoder may be created now. */
  mayRestart(): boolean {
    const last = this.#lastFaultAtMs;
    return last === undefined || this.#clock() - last >= this.#backoffMs;
  }
}
