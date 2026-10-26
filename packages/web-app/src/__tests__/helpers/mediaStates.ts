// File: packages/web-app/src/__tests__/helpers/mediaStates.ts
//
// A COMPLETE per-kind snapshot for web-app tests (the SDK only ever hands the
// app complete ones — `sdk-core/src/signaling/mediaKindStates.ts`). Every
// kind defaults to live and not server-muted; a test overrides what it pins.
//
// TWIN: `packages/sdk-svelte/src/__tests__/helpers/mediaStates.ts` is the same helper on purpose — no shared home is reachable
// (neither package depends on test-utils, which takes no sdk-core dependency).
// Change both together.

import { perMediaKind } from '@darktower/sdk-core';
import type { KindMediaState, ParticipantMediaStates, PublishKind } from '@darktower/sdk-core';

export function mediaStates(
  overrides: Partial<Record<PublishKind, Partial<KindMediaState>>> = {},
): ParticipantMediaStates {
  return perMediaKind((kind) => ({ clientState: 'live', serverMuted: false, ...overrides[kind] }));
}
