## Summary
Across 26 devloops the Ownership Lens was specific rather than templated in 24 records, produced seven classification upgrades and three ESCALATE routes, and never downgraded a tier. The two failures are records, not rulings: two devloops shipped `## Code Review Results` as the unfilled template, and one of those has no reviewer verdict anywhere in the file.
## Ownership Lens Verdict Audit
- Devloop: 2026-08-28-env-config-guard-per-instance-workloads   Classification: Minor-judgment   Outcome: clean
- Devloop: 2026-08-31-media-frame-v2-codec   Classification: Domain-judgment   Outcome: clean
- Devloop: 2026-08-31-signaling-media-contract-reshape   Classification: Minor-judgment   Outcome: upgraded (four Mechanical rows to Minor-judgment at Gate 1)
- Devloop: 2026-09-01-internal-contract-reshape   Classification: Domain-judgment   Outcome: clean
- Devloop: 2026-09-01-mh-transport-seam   Classification: Domain-judgment   Outcome: clean (Paired, owner-implemented)
- Devloop: 2026-09-01-release-premise-and-preflight-guards   Classification: Domain-judgment   Outcome: clean (NO Lens verdict recorded — section is the unfilled template)
- Devloop: 2026-09-02-media-path-slos-and-observability-policy   Classification: Domain-judgment   Outcome: upgraded (alerts.md hunk Mechanical to Minor-judgment)
- Devloop: 2026-09-02-frame-v2-cross-language-vectors   Classification: Domain-judgment   Outcome: clean (GSA intersection co-sign, media-handler added at Gate 1)
- Devloop: 2026-09-02-mh-transport-parameter-manifests   Classification: Minor-judgment   Outcome: upgraded (implementer proposed Mine; security raised it)
- Devloop: 2026-09-02-mc-kek-identity-sender-id   Classification: Domain-judgment   Outcome: escalated (F-INFRA-5, CLAUDE.md edit routed to the Lead)
- Devloop: 2026-09-02-mh-register-meeting-policy-apply   Classification: Domain-judgment   Outcome: clean (discharged on exact landed text)
- Devloop: 2026-09-02-mh-transport-config-and-drain   Classification: Minor-judgment   Outcome: clean
- Devloop: 2026-09-02-mc-media-routing-control-plane   Classification: Minor-judgment   Outcome: clean (three owners un-notified until a Gate-3 sweep; classifications stood)
- Devloop: 2026-09-03-mc-client-media-signaling   Classification: Domain-judgment   Outcome: upgraded (three mc-service manifest rows, security F2)
- Devloop: 2026-09-05-sdk-frame-v2-sframe-stack   Classification: Minor-judgment   Outcome: clean (GSA owner-implements; Lens discloses the missing second pair of eyes)
- Devloop: 2026-09-05-mh-audio-datagram-forward-path   Classification: Domain-judgment   Outcome: escalated (R-15 sender binding, mandated spin-out under ADR-0024 §6.3)
- Devloop: 2026-09-05-sender-id-binding-contract   Classification: Domain-judgment   Outcome: clean (protocol, media-handler and auth-controller co-signs)
- Devloop: 2026-09-07-media-telemetry-deny-guard   Classification: Domain-judgment   Outcome: upgraded (three rows Mechanical to Minor-judgment; one row to Domain-judgment)
- Devloop: 2026-09-08-media-telemetry-deny-policy   Classification: Mine   Outcome: clean
- Devloop: 2026-09-08-adr-0036-control-self-tests   Classification: Domain-judgment   Outcome: escalated (layer3.sh raised to Domain-judgment, auto-routed, Lead ruled for infrastructure)
- Devloop: 2026-09-08-sdk-audio-media-pipeline   Classification: Domain-judgment   Outcome: clean
- Devloop: 2026-09-08-mc-steer-client-to-edge-handler   Classification: Domain-judgment   Outcome: upgraded (TODO.md row under-scoped as Mine, split at Gate 3)
- Devloop: 2026-09-08-mh-datagram-receive-path-gap   Classification: Minor-judgment   Outcome: clean (test trailer granted verbatim)
- Devloop: 2026-09-09-web-app-in-meeting-loopback   Classification: Minor-judgment   Outcome: clean (Lens in the Gate-3 table; the Code Review Results section is the unfilled template)
- Devloop: 2026-09-09-media-path-runbooks-and-alerts   Classification: Domain-judgment   Outcome: clean (paired route for observability and infrastructure rows)
- Devloop: 2026-09-09-media-dashboards-and-catalogs   Classification: Domain-judgment   Outcome: upgraded (two rows Mechanical to Minor-judgment at Gate 1)
## Pattern Observations
- **Mechanical is where drift lives.** All seven upgrades moved off Mechanical or Mine and none moved down. The recurring trigger is the sed-test failing on prose: an owner-specified doc hunk, a list append whose second half adds an `options.labels` block, an alerts inventory that also adds normative guidance. The same edit shape was classified Mechanical early in the story and Minor-judgment late, which reads as the rule tightening rather than as inconsistency.
- **GSA was never routed as Mechanical**, and entries were specific rather than templated in 24 of 26 records. Every `proto/**` and `crates/media-protocol/**` row carried Domain-judgment or owner-implements, and the one intersection case added the co-owner to the roster at Gate 1 rather than asserting the co-sign on their behalf.
- **The Paired flag carried more weight than trailers, and the weak point is notification rather than classification.** Ten devloops used `--paired-with` or owner-implements; three state explicitly that a trailer alone would not have produced the owner's exact landed text. One devloop reached Gate 3 with three Minor-judgment rows whose owners had been sent nothing, found by the implementer's own diff-versus-table sweep. Of the three ESCALATE routes, two were classification disputes resolved on the reviewer's reasoning and the third was a compelled spin-out under §6.3.
## Follow-Ups
- Filed in `docs/TODO.md` §Process / Review-Protocol: a devloop can ship `## Code Review Results` as the unfilled template and no guard notices, with both instances, the severity split between them and the wrong-fix trap named. Owner infrastructure, operations co-sign. No separate entry for the un-notified-owner pattern — that section already carries the reconcile-against-the-diff item from `2026-09-02-mc-media-routing-control-plane`.
