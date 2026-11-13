// File: packages/sdk-core/src/media/closeCode.ts
//
// THE ONE disposition of how an MH WebTransport session ended (ADR-0039 §5,
// §7; numbers defined once in `signaling.proto` `MhCloseCode`). Exhaustive
// over the GENERATED enum with a `never` check, so a value added to the proto
// fails `tsc` here before any MH can emit it (the proto's EVOLUTION rule).
//
// Only `WebTransportCloseInfo.closeCode` is ever read. The close `reason`
// string is free-form server text: never read, never logged.
//
// NOT the MC signalling close code (`SignalingClient`'s auth close) — a
// different channel and a different concept.

import { MhCloseCode } from '../proto/dark_tower/signaling/v1/signaling_pb.js';
import type { MhCloseCodeToken, MhDialOutcome, MhDisconnectCause } from './setup/mediaMetrics.js';

/**
 * What the transport layer does about a session end.
 *
 * - `retry`: counted, NO loss report, backoff redial while the ref is in the set
 *   (WRONG_HANDLER, BINDING_CONFLICT, MC_UNAVAILABLE).
 * - `released`: expected after a set change (HANDLER_RELEASED).
 * - `declined`: "token dead", not "handler dead" (DECLINED): park, report FAILED.
 * - `loss`: transport loss (no clean close): loss report + backoff redial.
 * - `contract_fault`: a clean close carrying 0 or an unknown code — counted,
 *   surfaced once, and otherwise handled exactly as `loss`. Never fatal.
 */
export type CloseDisposition = 'retry' | 'released' | 'declined' | 'loss' | 'contract_fault';

/**
 * Classify a session end. `undefined` means NO code: the session ended without
 * a clean close (the caller maps a rejected `closed` to `undefined`).
 */
export function classifyClose(closeCode: number | undefined): CloseDisposition {
  if (closeCode === undefined) return 'loss';
  if (!isMhCloseCode(closeCode)) return 'contract_fault';
  return dispositionOf(closeCode);
}

function isMhCloseCode(value: number): value is MhCloseCode {
  // Numeric TS enum: the reverse mapping names exactly the generated values.
  return typeof (MhCloseCode as unknown as Record<number, unknown>)[value] === 'string';
}

function dispositionOf(code: MhCloseCode): CloseDisposition {
  switch (code) {
    case MhCloseCode.WRONG_HANDLER:
    case MhCloseCode.BINDING_CONFLICT:
    case MhCloseCode.MC_UNAVAILABLE:
      return 'retry';
    case MhCloseCode.HANDLER_RELEASED:
      return 'released';
    case MhCloseCode.DECLINED:
      return 'declined';
    case MhCloseCode.UNSPECIFIED:
      // Never sent (proto); a clean close that decodes to 0 is a fault.
      return 'contract_fault';
    default: {
      const unhandled: never = code;
      return unhandled;
    }
  }
}

/**
 * THE ONE projection of a CLEAN close's code onto its bounded telemetry token
 * (story 4 C2, R-42) — used for BOTH `dt_client_mh_connection_total{outcome}`
 * (a close inside the admission window) and `dt_client_mh_disconnects_total
 * {cause}` (a close of an established session). Tokens are the proto value name
 * with the `MH_CLOSE_CODE_` prefix stripped, lowercased — the same tokens MH's
 * `CloseCode::as_label()` uses — written out here (never computed from the
 * name, so the test's name-derivation is an independent check). A 0 or unknown
 * code is `contract_fault`.
 *
 * Takes a PRESENT code only: "no code" (a session lost without a clean close)
 * is not a close code, and each call site names its own token for it
 * (`transport_failure` for a dial, `transport_loss` for a disconnect).
 */
export function closeCodeToken(closeCode: number): MhCloseCodeToken {
  if (!isMhCloseCode(closeCode)) return 'contract_fault';
  switch (closeCode) {
    case MhCloseCode.WRONG_HANDLER:
      return 'wrong_handler';
    case MhCloseCode.BINDING_CONFLICT:
      return 'binding_conflict';
    case MhCloseCode.MC_UNAVAILABLE:
      return 'mc_unavailable';
    case MhCloseCode.HANDLER_RELEASED:
      return 'handler_released';
    case MhCloseCode.DECLINED:
      return 'declined';
    case MhCloseCode.UNSPECIFIED:
      return 'contract_fault';
    default: {
      const unhandled: never = closeCode;
      return unhandled;
    }
  }
}

/**
 * A DIAL's outcome when its provisional session closes inside the admission
 * window: the close-code token, or `transport_failure` when there was no clean
 * close (`undefined`).
 */
export function dialCloseOutcomeOf(closeCode: number | undefined): MhDialOutcome {
  return closeCode === undefined ? 'transport_failure' : closeCodeToken(closeCode);
}

/**
 * An ESTABLISHED session's disconnect cause: the close-code token, or
 * `transport_loss` when there was no clean close (`undefined`). The ONE input
 * of `isHandlerLoss` everywhere (the transport's `handlerLost` and the per-ref
 * reconnect episode).
 */
export function disconnectCauseOf(closeCode: number | undefined): MhDisconnectCause {
  return closeCode === undefined ? 'transport_loss' : closeCodeToken(closeCode);
}
