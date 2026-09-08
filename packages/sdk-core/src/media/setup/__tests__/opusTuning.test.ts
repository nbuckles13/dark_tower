// File: packages/sdk-core/src/media/setup/__tests__/opusTuning.test.ts
//
// `AudioEncoder.isConfigSupported` returns the config with UNRECOGNISED MEMBERS
// STRIPPED. `application` and `signal` were added to the WebCodecs Opus
// registration after the TypeScript DOM lib was cut, so a user agent that does
// not recognise them ignores them SILENTLY — leaving the encoder tuned for
// general audio while the code claims voice tuning.
//
// This comparison is what turns "did the voice tuning apply" from an assumption
// into an observation, so it is tested rather than left inside the file that
// cannot execute under `environment: 'node'`.

import { describe, expect, it } from 'vitest';

import { effectiveTuning } from '../opus.js';

const REQUESTED = { application: 'voip', signal: 'voice' } as const;

describe('effectiveTuning', () => {
  it('reports both applied when the user agent kept them', () => {
    expect(effectiveTuning(REQUESTED, { application: 'voip', signal: 'voice' })).toEqual({
      applicationApplied: true,
      signalApplied: true,
    });
  });

  it('reports NOT applied when the user agent stripped them', () => {
    // The silent-degradation case: an older Chrome drops the members it does not
    // know, and without this observation the encoder would be tuned for general
    // audio while every comment in the tree said `voip`.
    expect(effectiveTuning(REQUESTED, {})).toEqual({
      applicationApplied: false,
      signalApplied: false,
    });
  });

  it('reports them independently — one can survive without the other', () => {
    expect(effectiveTuning(REQUESTED, { application: 'voip' })).toEqual({
      applicationApplied: true,
      signalApplied: false,
    });
    expect(effectiveTuning(REQUESTED, { signal: 'voice' })).toEqual({
      applicationApplied: false,
      signalApplied: true,
    });
  });

  it('reports a member the caller never requested as APPLIED, not rejected', () => {
    // There is nothing for the user agent to have stripped, so "absent" must not
    // read as "rejected" — otherwise a config that deliberately omits a member
    // reports a degradation that did not happen.
    expect(effectiveTuning({}, {})).toEqual({ applicationApplied: true, signalApplied: true });
    expect(effectiveTuning(undefined, {})).toEqual({
      applicationApplied: true,
      signalApplied: true,
    });
  });

  it('reports a CHANGED value as not applied, not merely a missing one', () => {
    // A user agent that substitutes its own value has not honoured the request
    // either, and equality rather than presence is what catches it.
    expect(effectiveTuning(REQUESTED, { application: 'audio', signal: 'music' })).toEqual({
      applicationApplied: false,
      signalApplied: false,
    });
  });
});
