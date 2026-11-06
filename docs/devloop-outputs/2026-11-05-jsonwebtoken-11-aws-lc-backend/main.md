# Devloop Output: jsonwebtoken 11.1 — aws_lc_rs backend, fuzz drift, metrics-exporter defaults, ADR-0027 amendment

**Date**: 2026-11-05
**Task**: Make PR 101 (jsonwebtoken 9.3.1 -> 11.1.0) pass all layers: select aws_lc_rs backend, align fuzz crate, drop metrics-exporter-prometheus push-gateway default, amend ADR-0027
**Specialist**: auth-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `dependabot/cargo/jsonwebtoken-11.1.0`
**Duration**: ~2h (setup 19:55 → Gate 2 pass ~21:51 UTC; Gate 2 alone ~31m, L7 ~26m)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `2869bf3707cc5e8785f5900ac5ab98b9c5590dad` |
| Branch | `dependabot/cargo/jsonwebtoken-11.1.0` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (auth-controller)` |
| Implementing Specialist | `auth-controller` |
| Tier | `full` |
| Iteration | `2` |
| Security | `paired-security (--paired-with=security; replaces security slot)` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Infrastructure | `infrastructure (owner: dt-guard R3 machinery, Domain-judgment)` |
| Semantic Guard | `not spawned — diff is Cargo.toml/ADR prose, no checks.md surface` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Task Overview

### Objective
Make PR 101 (jsonwebtoken 9.3.1 -> 11.1.0) green on every layer without reverting the bump: select the security-decided `aws_lc_rs` backend, align the excluded fuzz workspace, drop `metrics-exporter-prometheus`'s default features (push-gateway is what puts `aws_lc_rs` into rustls), and amend ADR-0027 for the new approved backend.

### Scope
- **Service(s)**: all JWT consumers (AC sign/verify; GC/MC/MH/env-tests verify via `common::jwt`); every metrics-exporter consumer (AC/GC/MC/MH + test-utils)
- **Schema**: No
- **Cross-cutting**: Yes (workspace dependency features; ADR-0027 crypto policy)

### Debate Decision
NOT NEEDED - backend choice already decided by security (aws_lc_rs; rust_crypto rejected, ring not offered by jsonwebtoken >=10). Remaining work is completing that decision across every manifest/doc that encodes it.

---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `Cargo.toml` (`jsonwebtoken` entry + comment) | Not mine, Domain-judgment | security (paired; ADR-0027 crypto-primitive GSA, co-owned with auth-controller) |
| `Cargo.toml` (FEATURE UNIFICATION comment L71-73: jsonwebtoken backend is the deliberate exception) | Not mine, Domain-judgment | security (paired; states the crypto-provider policy) |
| `Cargo.toml` (`metrics-exporter-prometheus` entry + comment) | Not mine, Minor-judgment | observability |
| `Cargo.toml` (`getrandom-02` comment: drop "jsonwebtoken" from the ring-dependents list) | Not mine, Mechanical | infrastructure |
| `crates/ac-service/fuzz/Cargo.toml` (`jsonwebtoken` version + `aws_lc_rs`) | Not mine, Domain-judgment | security (paired; ADR-0027 crypto-primitive GSA) |
| `crates/ac-service/fuzz/Cargo.toml` (base64 comment: say what R3 actually checks) | Not mine, Mechanical | infrastructure |
| `crates/ac-service/fuzz/fuzz_targets/jwt_validation.rs` (HS256 -> EdDSA, fixed-seed public key) | Not mine, Domain-judgment | security (paired; GSA: ADR-0027 crypto-primitive call site) |
| `crates/ac-service/fuzz/common/jwt_seed_key.rs` (new, outside `fuzz_targets/` because `fuzz_wiring.rs` requires every file there to be a bin: the one fixed seed + key derivation, shared by target and generator) | Not mine, Domain-judgment | security (paired; GSA: ADR-0027 crypto-primitive call site) |
| `crates/ac-service/fuzz/examples/gen_jwt_seed.rs` (new: seed-token generator run at fuzz time into the gitignored `fuzz/corpus/jwt_validation/`; read-back self-check; iteration 2) | Not mine, Domain-judgment | security (paired; GSA: ADR-0027 crypto-primitive call site) |
| `.github/workflows/fuzz-nightly.yml` (iteration 2: per-target `seed_cmd`, incl. the 4 media-protocol `corpus_seeds` entries reviewed by protocol + media-handler; "Generate seed corpus" step, seeded-or-fail check, crash steps gated on `steps.fuzz`) | Not mine, Domain-judgment | infrastructure |
| `crates/media-protocol/tests/fuzz_wiring.rs` (iteration 2: every `fuzz/examples/*.rs` is the `seed_cmd` of exactly one matrix entry; every named example exists; GSA `crates/media-protocol/**`: co-owners protocol + media-handler, security required; the guard admits exactly one Owner name, so protocol is listed; test reviews intent) | Not mine, Minor-judgment | protocol |
| `docs/FUZZING.md` (iteration 2: §Seed Corpus ac-service `jwt_validation` local workflow + nightly `seed_cmd` wiring) | Not mine, Minor-judgment | test |
| `.dockerignore` (add `**/fuzz/`: the locally generated JWT seed, local corpora and crash artifacts must not enter the image build context; `validate-docker-context` scans untracked files too) | Not mine, Minor-judgment | infrastructure |
| `Cargo.lock` (regen) | Not mine, Minor-judgment | security (resolved crypto backend graph; derived, no hand edits) |
| `crates/dt-guard/src/common/cargo_manifest.rs` (`DepSpec` carries `features`; fail-closed parse) | Not mine, Domain-judgment | infrastructure (guard machinery) |
| `crates/dt-guard/src/workspace_deps.rs` (R3 features-superset + default-features match; tests) | Not mine, Domain-judgment | infrastructure (guard machinery + content) |
| `crates/dt-guard/tests/workspace_deps_e2e.rs` (R3 features positive control) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/src/resolved_features/mod.rs` (new G2 machinery: cargo metadata invocation, parse, evaluator) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/src/resolved_features/policy.rs` (new G2 policy data: provider partitions + aws-lc reverse-dependent backstop) | Not mine, Domain-judgment | security (GSA: ADR-0027 crypto primitives; content co-owned with auth-controller) |
| `crates/dt-guard/src/lib.rs` (register `resolved_features`) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/main.rs` (dispatch `resolved-features`) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/common/cargo_invoke.rs` (new: policy-free `run_cargo(repo_root, args)` — `$CARGO` fallback `cargo`, `current_dir`, stderr head on failure; separate from `cargo_cli.rs`, which parses cargo command TEXT and spawns nothing) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/src/common/mod.rs` (register `cargo_invoke`) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/service_dep_exclusion.rs` (port its cargo invocation onto `common::cargo_invoke::run_cargo`) | Not mine, Domain-judgment | infrastructure |
| `crates/dt-guard/tests/resolved_features_e2e.rs` (new: bogus `--root` precondition + real-tree PASS) | Not mine, Domain-judgment | infrastructure |
| `scripts/lang/rust/compile.sh` (wire `resolved-features` in Layer 1 after `service-dep-exclusion`) | Not mine, Domain-judgment | infrastructure |
| `docs/runbooks/devloop-validation.md` (§6.1 row `resolved-features-failed`) | Not mine, Minor-judgment | operations |
| `docs/specialist-knowledge/infrastructure/INDEX.md` (pointer to the new guard) | Not mine, Mechanical | infrastructure |
| `docs/specialist-knowledge/auth-controller/INDEX.md` (pointer: ADR-0027 backend + enforcing policy file) | Mine | — |
| `docs/decisions/adr-0027-approved-crypto.md` (operator row + line 39 verbatim; separate "Enforced by" line pointing at `resolved_features/policy.rs`) | Not mine, Domain-judgment | security (paired; ADR-0027 policy owner) |
| `docs/BUILD_REQUIREMENTS.md` (JWT now via aws-lc-rs, not ring; every "JWT" hit) | Not mine, Mechanical | infrastructure |
| `docs/TODO.md` (§Media Path Obligations: fold this PR's evidence + lane constraints into the existing "Neither fuzz workspace is compiled" entry; §Observability Debt: run_upkeep entry written by @observability; §Cross-Service Duplication: `fuzz/common/jwt_seed_key.rs` added as 8th TestKeypair location by @dry-reviewer; §Observability Debt: N/A-clobber evidence appended to the concurrent-`layer-all.sh` entry per @test) | Not mine, Minor-judgment | test + infrastructure; observability; dry-reviewer |

Not edited: `crates/common/src/jwt.rs`, `crates/ac-service/src/crypto/**` (GSAs). `cargo check --workspace --all-targets` at the PR head is warning-free, so there is no compile-level API breakage. Any behavioural breakage would show in L4/L5 and would be fixed there with @paired-security. Historical `docs/devloop-outputs/**`, `docs/debates/**`, `docs/user-stories/**` mentions of jsonwebtoken are records, not live references.

---

## Planning

### Mechanism restatement
The task frames this as "jsonwebtoken needs a backend". The mechanism is wider: **every crate in the graph that picks a process-level crypto provider from crate features must see exactly one provider feature, in every Cargo workspace that compiles it, and every place that repeats that selection must be drift-guarded.** Two crates in our graph do this:

| Crate | Today (PR head) | Failure | After this change |
|-------|-----------------|---------|-------------------|
| `jsonwebtoken` 11.1 (`CryptoProvider::from_crate_features`) | zero providers (root `"11.1"`, defaults = `use_pem` only; fuzz `default-features = false`) | panics on every sign/verify (loud: L4/L5, see "Test evidence") | exactly `aws_lc_rs`, root and fuzz, with the fuzz copy bound by R3 (features) |
| `rustls` 0.23.46 (`CryptoProvider::get_default_or_install_from_crate_features`) | **both** `ring` and `aws_lc_rs`. `aws_lc_rs` arrives only via `metrics-exporter-prometheus` default `push-gateway` -> `hyper-rustls/aws-lc-rs` (`cargo tree -e features -i rustls`); this predates PR 101 | latent: any `rustls::{Client,Server}Config::builder()` without an explicit provider would panic. No current site does: wtransport `ring` feature -> `rustls::crypto::ring::default_provider()` + `builder_with_provider` (`wtransport-0.7.2/src/tls.rs:607-617`); reqwest `__rustls-ring` -> `ring::default_provider()` when no process default (`reqwest-0.12.28/src/async_impl/client.rs:763-770`); no direct `rustls::` use in `crates/`; no `install_default` anywhere | exactly `ring`, so ADR-0027's "(runtime TLS currently uses rustls' ring provider)" holds by construction, not only at today's call sites |

So the metrics-exporter edit is the second half of the same invariant. The R3 extension is the drift guard on its one repeated copy.

### Facts verified from source / tree
- **Key loading**: every jsonwebtoken key is loaded via `EncodingKey::from_ed_der` (20 sites), `DecodingKey::from_ed_der` (13) or `*::from_secret` (2 test sites, `mh-service/tests/common/tokens.rs:160` and `gc-service/tests/meeting_tests.rs:332`, plus the fuzz target, which this PR switches to EdDSA). There is no `from_*_pem`, no `jsonwebtoken::jwk` and no `dangerous::*`. `use_pem` is unused, so root `default-features = false` is safe.
- **9->11 API changes per call-site file**: none at the compile level. `cargo check --workspace --all-targets` at the PR head finishes with zero warnings and errors. That covers `ac-service/src/crypto/mod.rs`, `services/token_service.rs`, `handlers/internal_tokens.rs`, `common/src/jwt.rs`, the gc/mc/mh auth layers and env-tests (`decode_header`, `Algorithm` compares; `Algorithm` being `non_exhaustive` breaks nothing because we never match on it exhaustively). The behavioural changes, confirmed by @paired-security against the 11.1 source: the verifier and signer constructors check key family (alg-confusion closed harder than in 9.x), and `Validation` defaults are unchanged. No `CryptoProvider::install_default` or wrapper is needed and none is added.
- **EdDSA path in 11.1 (`src/crypto/aws_lc/eddsa.rs`)**: the signer is `aws_lc_rs::signature::Ed25519KeyPair::from_pkcs8`, called per `encode`. AC already validates the PKCS#8 with `ring::Ed25519KeyPair::from_pkcs8` first (`ac-service/src/crypto/mod.rs:206`); keys come from `ring::generate_pkcs8`, v2. The verifier is `aws_lc_rs::signature::ED25519.verify_sig` over the raw 32-byte key. Strictness matches ring (BoringSSL lineage, S < L, cofactorless), confirmed by @paired-security. Ed25519 is deterministic, so tokens are byte-identical across backends and mixed-version AC/GC/MC is safe.
- **Algorithm pinning unchanged**: every validation site uses `Validation::new(Algorithm::EdDSA)` (`common/src/jwt.rs:1033`, `ac-service/src/crypto/mod.rs:272,484`).
- **metrics-exporter-prometheus 0.18.3** (confirmed by @observability): every API we call is ungated (`PrometheusBuilder::new`, `set_buckets_for_metric`, `Matcher`, `install_recorder`, `build_recorder`, `PrometheusHandle::render`). We call none of the gated items. Every `/metrics` endpoint is our own axum route calling `handle.render()` (AC `routes/mod.rs:273`, GC `handlers/metrics.rs:29`, MC `main.rs:391`, MH `main.rs:445`). So: `default-features = false` and **no features** (not `http-listener`, `uds-listener`, `async-runtime` or `protobuf`). Upkeep behaviour is identical: `install_recorder` never spawns upkeep, with or without the features.
- **aws-lc-sys build**: aws-lc-sys 0.45.0 is already in `Cargo.lock` and compiled into every service image today (via push-gateway), built with gcc only (rust:*-slim-bookworm, devloop image, ubuntu-latest). ac/gc/mc/mh all reach jsonwebtoken as a normal dependency through `common`, so no service gains or loses aws-lc-sys and image build time does not change (confirmed by @operations). No cmake, clang or nasm is added anywhere.
- **Other encodings of the jsonwebtoken entry**: root `Cargo.toml` (entry, plus the L71-73 "NOT unified ... crypto provider" comment and the L192 getrandom comment), `crates/ac-service/fuzz/Cargo.toml`, `Cargo.lock`, and `docs/BUILD_REQUIREMENTS.md` (L22, L24, L212, L484). It is not present in `.github/dependabot.yml`, `audit-suppressions.toml`/`.cargo/audit.toml`, `crates/media-protocol/fuzz` or any dt-guard policy list. There is no cargo-deny config.

### Test evidence (answers to @test)
1. **Which tests catch the missing backend** (local run on the unfixed PR head):
   - `cargo test -p common --lib` gives **8 FAILED**, all panicking at `jsonwebtoken-11.1.0/src/crypto/mod.rs:124`: `jwt::tests::test_verify_token_roundtrip_with_real_keys`, `test_verify_token_roundtrip_user_claims`, `test_verify_token_rejects_wrong_key`, `test_verify_token_accepts_jwk_without_alg_field`, `test_jwt_validator_validate_roundtrip`, `test_jwt_validator_validate_user_claims_roundtrip`, `test_jwt_validator_rejects_future_iat`, `test_jwt_validator_rejects_expired_token`.
   - `cargo test -p ac-service --lib crypto` gives **19 FAILED** (`test_jwt_sign_verify`, `test_sign_user_jwt_valid_jwt`, `test_verify_jwt_tampered_token`, ...).
2. **Named controls (@test A).** The common-side control is `crates/common/src/jwt.rs:2808` `test_verify_token_accepts_jwk_without_alg_field`: a real EdDSA encode (`from_ed_der`) plus verify, run under `cargo test -p common` alone. It is red at `2869bf37`, panicking at `jsonwebtoken-11.1.0/src/crypto/mod.rs:125` (observed above). The AC-side signer control is `crates/ac-service/src/crypto/mod.rs:532` `test_jwt_sign_verify`, also red at `2869bf37`. Both green runs get pasted after the fix. Neither needs a new test.
   **Superseded wording, kept for the record**: the test already exists. `crates/common/src/jwt.rs:2858` `test_verify_token_roundtrip_with_real_keys` (real EdDSA encode + `verify_token` decode) runs under `cargo test -p common` alone. That `-p common` run went red on the unfixed head, which is the "feature removed" state, so it does not depend on unification from another member. That run is the positive control. After the fix I'll re-run it green and paste both outputs. No new test is added because it would duplicate this one.
3. **R3 features gap**: in scope (Lead ruling). Design in "R3 extension" below.
4. **Fuzz build**: see Edit 9. No Layer 1-6 lane compiles the excluded fuzz workspaces. The only per-PR contacts are R3 (manifest text) and `audit.sh` (`cargo generate-lockfile`, resolution only). That is why the 9.3-vs-11.1 drift surfaced only through R3. The lane is sized under "Deferred, with reasons".
5. **/metrics still works**: no in-process HTTP scrape test exists. The coverage is:
   - per-service `metrics_module_emits_*` tests and `ac-service/src/routes/mod.rs` `test_init_metrics_recorder_does_not_panic`, which build or install the recorder and `render()`, at L4/L5;
   - the real scrape at L7, `crates/env-tests/tests/30_observability.rs:37` `test_all_services_scraped_by_prometheus`.
   The feature change cannot affect a `render()` path, which is ungated.

### Edits
1. **Root `Cargo.toml` jsonwebtoken**: `jsonwebtoken = { version = "11.1", default-features = false, features = ["aws_lc_rs"] }`. The short comment says:
   - the backend is selected here once, per ADR-0027;
   - exactly one of `aws_lc_rs`/`rust_crypto` must be on, because with zero or both every sign/verify panics (`CryptoProvider::from_crate_features`);
   - members stay a bare `{ workspace = true }` with no per-crate `features`;
   - `use_pem` is off because all keys are DER;
   - the excluded fuzz crate repeats this entry and R3 binds version, defaults and features.
   Inline, mechanical facts only: exactly one provider; zero or two panic; `rust_crypto` is not enabled; `use_pem` is dropped; the fuzz crate repeats this entry and R3 enforces it; G2 (`dt-guard resolved-features`) enforces the resolved result. Then "rationale: ADR-0027". The why is not restated (@dry-reviewer, @paired-security).
2. **Root `Cargo.toml` L71-73** FEATURE UNIFICATION comment: rewrite the provider clause so it says what is now true. Crypto-provider selection IS pinned once, at workspace level: jsonwebtoken's backend sits on its `[workspace.dependencies]` entry, and rustls is held to ring by the metrics-exporter's defaults being off. The reason is that each provider-selecting crate needs exactly one provider across the whole build. `dt-guard resolved-features` enforces it; rationale: ADR-0027. The `common` `db` clause is unchanged.
3. **Root `Cargo.toml` metrics-exporter-prometheus**: `{ version = "0.18", default-features = false }`. The comment says:
   - we install via `install_recorder`/`build_recorder` and serve `/metrics` from our own axum routes, so no exporter feature is needed;
   - the `push-gateway` default would put `aws_lc_rs` into rustls alongside `ring` (two rustls providers; see ADR-0027's TLS line, not re-argued here);
   - `http-listener` is unused.
4. **Root `Cargo.toml` L192** getrandom comment: "ring (via rustls/quinn)".
5. **`crates/ac-service/fuzz/Cargo.toml`**: `version = "11.1"`, keep `default-features = false` (operator requirement), add `features = ["aws_lc_rs"]`, with a note "mirrors root [workspace.dependencies]; dt-guard workspace-deps R3 checks version/defaults/features". The existing base64 comment says R3 checks the mirror, and today R3 ignores its `features = ["std"]`. After the R3 change that claim is true; the comment is reworded to name version/defaults/features explicitly.
6. **`fuzz_targets/jwt_validation.rs`** (@paired-security 4b): replace `DecodingKey::from_secret(&[0;32])` + `Validation::new(Algorithm::HS256)` with a real Ed25519 public key and `Validation::new(Algorithm::EdDSA)`, keeping `validate_exp = false`.
   - The key is derived once (`std::sync::OnceLock`) from a fixed seed via `ring::signature::Ed25519KeyPair::from_seed_unchecked(&[0x42; 32])`, then `.public_key()` and `DecodingKey::from_ed_der`.
   - `ring` is already a dependency of the fuzz crate.
   - It is a real curve point, so verify reaches the curve math, and no key bytes are hardcoded (no secret-guard noise). The Lead accepted the deterministic fuzz seed: it is reproducible and not a credential. It verifies nothing outside the harness.
   - **One home for the seed**: `common/jwt_seed_key.rs` holds `SEED`, `decoding_key()`, `encoding_key()` (ring's `from_seed_unchecked` exports no PKCS#8, so this wraps the seed in the RFC 8410 PKCS#8 v1 DER with its fixed 16-byte prefix `302e020100300506032b657004220420`. aws-lc-rs `Ed25519KeyPair::from_pkcs8` accepts v1, per `aws-lc-rs-1.18.1/src/ed25519.rs:378-383`. Only the generator calls it) and the `Claims` struct. Target and generator both pull it in via `#[path]`, so they cannot drift.
6b. **Seed corpus (@test B, @paired-security, Lead).** **[Superseded in iteration 2: no seed is committed; `gen_jwt_seed` writes it at fuzz time into the gitignored `fuzz/corpus/jwt_validation/`. See "Human Review (Iteration 2)".]** 
   - (iteration 1; superseded) `examples/gen_jwt_seed.rs` is run with `cargo run --example gen_jwt_seed` in `crates/ac-service/fuzz`. It signs `Claims { sub: "fuzz-seed", exp: 4102444800 }` with `Header::new(EdDSA)`, writes `seeds/jwt_validation/valid_eddsa`, then **decodes it back with the target's exact `decoding_key()` + `Validation`, and asserts `Ok`**. That self-check is evidence (i). Ed25519 is deterministic, so re-running reproduces the file byte-for-byte; the target's header comment documents the command.
   - (iteration 1; superseded) **The corpus location is forced by `.gitignore`**: `**/fuzz/corpus/` is ignored, and a negation cannot re-include files under an excluded directory. So the committed seed goes in `fuzz/seeds/<target>/`.
   - (iteration 1; superseded) **Wire-in**: today `fuzz-nightly.yml` runs `cargo +nightly fuzz run <target>` with the default corpus only, so the seed would be vacuous unless wired. The workflow edit: when `fuzz/seeds/<target>` exists, run `mkdir -p fuzz/corpus/<target>` and pass both dirs with the corpus dir FIRST, `cargo +nightly fuzz run <target> fuzz/corpus/<target> fuzz/seeds/<target> -- ...`. libFuzzer writes new inputs to the first dir, so the artifact upload is unchanged. Targets without a seeds dir are unchanged. Infrastructure reviews this: a workflow edit, action SHAs untouched.
   - (iteration 1; superseded) **Evidence (ii)**: with `aws_lc_rs` removed locally from the fuzz manifest, `cargo +nightly fuzz run jwt_validation fuzz/seeds/jwt_validation -- -runs=1` crashes at `crypto/mod.rs:124`. Then restore and re-run green. Both outputs get pasted.
7. **`Cargo.lock`**: re-resolved by cargo; no `cargo update`. Expected diff:
   - jsonwebtoken gains `aws-lc-rs` and loses `pem`/`simple_asn1`;
   - metrics-exporter loses its hyper/hyper-rustls/ipnet edges;
   - packages used only by push-gateway or http-listener drop out.
   **Exactly one** aws-lc-rs and one aws-lc-sys version, and no `cmake` crate.
8. **ADR-0027** (operator text, verbatim).
   - New Approved Algorithms row placed under `Signatures`: `| JWT signing/verification (EdDSA) | Ed25519 | jsonwebtoken feature `aws_lc_rs` (aws-lc-rs) | jsonwebtoken >=10 has no ring backend; `rust_crypto` backend not approved (non-strict dalek verify, pulls rsa / RUSTSEC-2023-0071) |`. Four columns, matching the table.
   - Line 39 replaced verbatim with the operator sentence (claim verified above).
   - Existing `Signatures` row: the Notes cell is minimally scoped so it no longer reads as covering JWT ("EdDSA, 256-bit security; direct use incl. key generation — JWT sign/verify: row below").
   - "Last reviewed" set to 2026-11-05.
   - The backend policy is stated once, in the table row. The guideline bullet is the operator's scoped exception, not a second copy. No "FIPS" claim.
9. **`docs/BUILD_REQUIREMENTS.md`**: fix every JWT hit.
   - L22: ring is used for TLS and direct crypto; JWT sign/verify goes through aws-lc-rs inside jsonwebtoken.
   - L24, L212: drop jsonwebtoken from ring's "Used by".
   - L484: "aws-lc-rs EdDSA".
   - Add a short `aws-lc-rs` / `aws-lc-sys` entry: needs a C compiler, prebuilt bindings on linux x86_64/aarch64, no cmake needed there.
   - Refer to ADR-0027 for the policy instead of restating the rationale.
10. **Fuzz build** (Lead and @test #4), run in `crates/ac-service/fuzz`:
    - install `cargo-fuzz` 0.13.2 (the pin in `infra/cargo-tools.versions`) with `--locked` into a scratch `CARGO_INSTALL_ROOT`;
    - run `cargo +nightly fuzz build jwt_validation`, which uses ASan and sancov, matching `fuzz-nightly.yml`;
    - (iteration 1; superseded) run `cargo +nightly fuzz run jwt_validation fuzz/corpus/jwt_validation fuzz/seeds/jwt_validation -- -runs=10000`, the seeded run against the EdDSA target;
    - also run `cargo check` for `media-protocol/fuzz` (Lead), since R3 now evaluates its deps too.
    The outcome gets reported loudly either way. If the cargo-fuzz install fails, I say so and fall back to the equivalent `RUSTFLAGS="-Cpasses=sancov-module ... -Zsanitizer=address --cfg fuzzing" cargo +nightly build`, labelled as that rather than as a cargo-fuzz run. If aws-lc-sys fails under the sanitizer flags, I raise it to @team-lead and do not work around it.

### R3 extension (dt-guard workspace-deps; infrastructure-owned machinery, Lead ruling: in scope)
- **One parser.** `DepSpec::Version` gains `features: BTreeSet<String>` (`common/cargo_manifest.rs:94`), and so does `DepSpec::WorkspaceInherit` for shape symmetry, though no rule reads it there. `classify_dep` fills it: an empty set for the string form, otherwise the table's `features` array. `WsDep` carries it. `release_build_profile.rs` gets `..` only where it destructures. No second parser.
- **Fail closed on malformed input.** If `features` is not an array, or holds a non-string element, `classify_dep` returns a new `DepSpec::Malformed { detail }` and never an empty set. A non-bool `default-features`/`default_features` takes the same route, since it is the same fail-open shape @infrastructure flagged. `workspace_deps` maps `Malformed` to a precondition-class hit:
  - on the root side, `WORKSPACE_MANIFEST_UNREADABLE`, so R3 cannot pass vacuously;
  - on a member or excluded side, `MEMBER_MANIFEST_UNREADABLE`.
  Every other `DepSpec` match treats `Malformed` as a precondition, never as `Other`.
- **Semantics.** `default` is modelled as a pseudo-feature: `effective = features ∪ {"default" if default_features != Some(false)}`. R3 then requires:
  - (a) **`excluded_effective ⊇ root_effective`**. This covers missing features. It also closes the case where the root keeps defaults and the excluded crate drops them, which today's "allowed direction" permitted and which can drop a runtime-needed feature just like a missing `aws_lc_rs`.
  - (b) **The existing rule "root disables defaults ⇒ excluded must too" is kept.** It is the opposite direction (excluded must not re-enable, e.g. base64 `simd-unsafe`), and a superset check cannot express it, so it does not replace it.
  Net effect: default-features must match the root exactly, and explicit features must be a superset. Extra features on the excluded side remain allowed. Real tree, checked by hand against both excluded crates:
  - ac-service/fuzz: `libfuzzer-sys`, `ring`, `serde` and `base64` already match; `jsonwebtoken` matches once root sets `default-features = false`, which keeps the operator's fuzz `default-features = false`.
  - media-protocol/fuzz: `libfuzzer-sys` and `bytes` match.
  So nothing fires and no manifest needs weakening. The R3 doc row (module doc L13) and the rule doc comment are updated to say version + default-features + features.
- **Message.** One hit per dependency, listing missing features sorted and backticked and naming both sides: "[dependencies] `jsonwebtoken` is missing features [`aws_lc_rs`] that root [workspace.dependencies] enables — this crate cannot inherit, so it must enable them too". A missing pseudo-feature `default` is reported as "disables default features that root keeps". The token stays `excluded_workspace_version_drift`.
- **Tests.**
  - Unit tests in `workspace_deps.rs` `mod tests`:
    - (a) a missing feature gives one hit whose detail names it;
    - (b) two missing features give one hit listing both, sorted;
    - (c) an excluded superset passes;
    - (d) a root string-form entry passes against anything;
    - (e) a `package =` rename and a `[target.'cfg(..)'.dependencies]` table are still matched;
    - (f) a malformed `features` on either side is a precondition, never a pass;
    - (g) the default direction both ways: root keeps defaults and excluded disables fires; root disables and excluded disables passes. The existing `r3_version_and_default_features_drift` "allowed direction" assertion is flipped accordingly, and its comment says why.
  - Real-tree positive control, also a unit test: `include_str!` the real root `Cargo.toml` and the real `crates/ac-service/fuzz/Cargo.toml`, run R3 on the real manifest and expect **0 hits**, then run it on the same text with `"aws_lc_rs"` removed and expect **1 R3 hit naming `aws_lc_rs`**. This binds the guard to the actual files at compile time.
  - `cargo_manifest.rs` tests for the new field and the `Malformed` variant.
  - `workspace_deps_e2e.rs`: an `r3_positive_control_features` alongside `r3_positive_control_default_features`.

### Verification steps (beyond layer-fast)
Before/after outputs get pasted into main.md:
- `cargo tree -e features -i rustls --workspace`: after the change, it must have no `aws_lc_rs`/`aws-lc-rs`.
- `cargo tree -i aws-lc-rs -e normal --workspace`: only path via jsonwebtoken.
- `grep -c 'name = "aws-lc-sys"\|name = "aws-lc-rs"' Cargo.lock`: 1 each; no `name = "cmake"`.
- `cargo test -p common --lib jwt` and `cargo test -p ac-service --lib crypto`: green after the change (red before, see "Test evidence").
- The fuzz build/run outputs (Edit 10), the seed self-check (6b i) and the zero-provider crash on the seed (6b ii).
- `dt-guard resolved-features`: FAIL on the `2869bf37` manifests and PASS after (G2 adverse pair).
- `cargo check --manifest-path crates/media-protocol/fuzz/Cargo.toml`.

### G2: `dt-guard resolved-features` (Lead ruling: in scope)
**Invariant:** each provider-selecting crate in the root workspace's resolved graph has exactly its approved provider features, and aws-lc is reachable only through jsonwebtoken.

- **Rule / subcommand**: `dt-guard resolved-features`.
  - Machinery goes in the new module `crates/dt-guard/src/resolved_features/mod.rs` (infrastructure). Policy data goes in `resolved_features/policy.rs` (security + auth-controller content; ADR-0027 GSA). The split is a separate file so ownership follows the path.
  - Data shape: `ProviderPolicy { krate, vocabulary: &[&str], required: &[&str], reason }` and `ReverseDepPolicy { krate, allowed_dependents: &[&str], reason }`. Both have a header doc comment naming ADR-0027 as the authority and the content owners, like `SINGLE_VERSION_CRATES`.
  - The check per resolved node is `resolved_features ∩ vocabulary == required`. It is a partition check, so additive features (`std`, `tls12`, `use_pem`, quinn's `rustls` alias) never matter.
- **Policy content**: vocabularies were read from the crate manifests in `/tmp/cargo-home/registry`, not taken from the review messages.

  | Crate (locked version) | Vocabulary | Required | Today at `2869bf37` (`cargo metadata` resolve) |
  |---|---|---|---|
  | `jsonwebtoken` 11.1.0 | `aws_lc_rs`, `rust_crypto` | `aws_lc_rs` | `default, use_pem`: ∩ = ∅, **FAIL** |
  | `rustls` 0.23.46 | `ring`, `aws_lc_rs`, `aws-lc-rs`, `fips`, `prefer-post-quantum` | `ring` | `aws-lc-rs, aws_lc_rs, ring, std, tls12`: **FAIL** (names `aws-lc-rs`, `aws_lc_rs`) |
  | `rustls-webpki` 0.103.16 | `ring`, `aws-lc-rs`, `aws-lc-rs-fips`, `aws-lc-rs-unstable` | `ring` | `alloc, aws-lc-rs, ring, std`: **FAIL** |
  | `quinn` 0.11.12 | `ring`, `rustls-ring`, `aws-lc-rs`, `aws-lc-rs-fips`, `rustls-aws-lc-rs`, `rustls-aws-lc-rs-fips`, `__rustls-post-quantum-test` | `ring`, `rustls-ring` | `ring, rustls, rustls-ring, ...`: pass |
  | `quinn-proto` 0.11.19 | same seven | `ring`, `rustls-ring` | `ring, rustls-ring, ...`: pass |

  Backstop (fail-closed against a future provider feature name we don't list, and enforcing ADR-0027's "do not call aws-lc-rs directly"):
  - `aws-lc-rs` reverse dependents ⊆ {`jsonwebtoken`}. Today: {`rustls`, `rustls-webpki`}, **FAIL**.
  - `aws-lc-sys` reverse dependents ⊆ {`aws-lc-rs`}. Today: pass.

  Quinn's required set is {`ring`, `rustls-ring`}, the one-rule-shape option @paired-security offered. Every entry carries a one-line `reason`, e.g. "rust_crypto: non-strict dalek verify, pulls rsa (RUSTSEC-2023-0071); see ADR-0027".
- **How it reads resolved features**: through the new shared `common::cargo_invoke::run_cargo(repo_root)` helper (`$CARGO`, fallback `cargo`, `current_dir(repo_root)`). `service_dep_exclusion` is ported onto the same helper (@infrastructure), and flags stay per call site: with `metadata --format-version 1 --offline --locked --manifest-path <root>/Cargo.toml`. Never cwd. No `--filter-platform`. Default features, not `--all-features`. Dev-deps are included because `resolve` covers them.
  - Parsing uses `serde_json` (already a dependency) into minimal typed structs: `packages[{id,name,version,features}]` and `resolve.nodes[{id,features,deps[{pkg}]}]`. No `cargo_metadata` crate.
  - Measured: 0.195 s warm on this tree.
  - Every resolved node of a policy crate is checked, across multiple versions, and each version is named in the detail.
- **Doc comment**:
  - *Scope*: root workspace only. The fuzz workspaces have no tracked lock, so `--offline --locked` cannot resolve them; their jsonwebtoken features are bound to root by R3's superset check.
  - *Union view*: the all-member, all-platform union over-approximates. That is fail-closed for the deny side. On the require side a single binary could lack a feature the union shows, which is covered by the feature living on the `[workspace.dependencies]` entry plus R1/R5 forcing inheritance (members can only add, never remove).
  - *Residual*: a provider feature added upstream that is absent from our vocabulary is invisible to the partition check. The aws-lc reverse-dependent backstop catches any such feature that pulls aws-lc. One that pulls a third backend is not caught, and the comment says so.
- **Fail loud.** These are PRECONDITION lines, each with its own REASON token, never a pass:
  - `resolved-features-cargo-failed`: non-zero exit, with the first stderr lines.
  - `resolved-features-metadata-unparseable`: bad JSON, `resolve` null, or zero packages.
  - `resolved-features-policy-crate-absent`: this is the positive control against renames and against a vacuous metadata run.
  - `resolved-features-vocabulary-term-undeclared`: a vocabulary term not in that package version's `packages[].features`, which catches upstream renames.

  Violations are `VIOLATION: [provider-features-mismatch]` (detail: crate@version, resolved∩vocab vs required, the offending terms sorted) and `VIOLATION: [reverse-dependent-disallowed]`.
- **Pipeline wiring: Layer 1, not Layer 3. This is a deliberate deviation from the "L3 wrapper" wording; @team-lead / @infrastructure, please confirm.** `scripts/layer3.sh:32-39` and `scripts/lang/rust/compile.sh` state "Layer 3 invoking cargo ZERO times is structural". The precedent `service-dep-exclusion`, which runs `cargo tree`, is wired in `compile.sh` for exactly that reason (`service_dep_exclusion.rs` §"Why this runs in Layer 1"). A `scripts/guards/simple/*.sh` wrapper would be auto-discovered into L3 and break that invariant. So: `run_and_emit "resolved-features" "${DT_GUARD:-$DT_GUARD_BIN}" resolved-features --root "$__repo_root"` right after `service-dep-exclusion`, no wrapper, plus a §6.1 row `resolved-features-failed` in `docs/runbooks/devloop-validation.md`.
- **Tests.**
  - Unit tests are hermetic: a pure `evaluate(raw, &[ProviderPolicy], &[ReverseDepPolicy])` over in-test JSON fixtures built with `serde_json::json!` (no fixture files), with no live cargo. Fixtures cover:
    - jsonwebtoken {} fires; {aws_lc_rs, rust_crypto} fires; {rust_crypto} fires;
    - rustls {ring, aws_lc_rs} fires, **named as `main`'s state at 2869bf37**; {aws_lc_rs} fires; {} fires;
    - webpki {ring, aws-lc-rs} fires;
    - quinn missing `rustls-ring` fires;
    - a new reverse dependent of aws-lc-rs (e.g. a workspace member) fires;
    - PASS: jsonwebtoken {aws_lc_rs, use_pem} + rustls {ring, std, tls12, logging} + webpki {ring, alloc, std} + quinn/quinn-proto good;
    - two rustls instances, one good and one bad, fire and name the bad version;
    - policy crate absent gives a precondition; vocabulary term undeclared gives a precondition; unparseable JSON and empty packages give preconditions; the cargo-failed path gives a precondition (via the injectable runner, like `service_dep_exclusion::evaluate`'s `resolve` closure).
    - A policy-sanity test checks that every policy entry's `required ⊆ vocabulary` and that vocabularies are non-empty.
  - e2e `crates/dt-guard/tests/resolved_features_e2e.rs`: bogus `--root` (missing manifest) gives `ERROR: PRECONDITION` with `resolved-features-cargo-failed`, and the real repo gives `STATUS=OK` after the fix.
  - **Adverse real-tree pair**: run the guard on the tree with only the dt-guard changes applied (manifests still as at `2869bf37`) and expect FAIL on jsonwebtoken, rustls, rustls-webpki and the aws-lc-rs backstop. Then with the manifest edits, expect PASS. Both outputs get pasted.

### Deferred, with reasons (pointers go in Accepted Deferrals)
- **Per-PR compile lane for the excluded fuzz workspaces.** Accepted by @team-lead and @infrastructure. No new TODO entry: the existing `docs/TODO.md` §Media Path Obligations entry "Neither fuzz workspace is compiled by the pipeline…" (owner test + infrastructure, story task 23) is this gap. I amend it with two things:
  - this PR's evidence: the 9.3-vs-11.1 drift surfaced only via R3, and a zero-provider fuzz build is caught only nightly;
  - the design constraints: it would be the first unlocked, crates.io-resolving build in L1-6 against `CARGO_LOCKED`, it needs network in containers, it needs `audit.sh`-style save/restore of local fuzz locks, it needs discovery from the root `exclude` list, it needs a `.test.sh`, and libfuzzer-sys's C++ build cost lands in L1.
- G2 is **not** deferred. The earlier "G2 to TODO" line and its reviewer-acceptance attribution are withdrawn, superseded by the Lead's ruling.

### Rollback
Whole-commit revert only. A partial revert is unsafe:
- reverting only the backend line re-breaks every JWT;
- reverting only the metrics-exporter line re-adds rustls's second provider, and G2 reds;
- reverting only the R3 or G2 changes leaves the backend selection unguarded. There is no schema, wire, config or env change, and no backend-selection knob.

### Security Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| jsonwebtoken backend | `aws_lc_rs` (root + fuzz) | Decided by security. jsonwebtoken >=10 has no ring backend. `rust_crypto` was rejected (non-strict ed25519-dalek verify; pulls `rsa`, RUSTSEC-2023-0071). Staying on 9.x only defers it. |
| Backend selection location | root `[workspace.dependencies]` only; members bare `{ workspace = true }`; fuzz mirror bound by R3 features | Exactly-one-backend is graph-wide, so one SSoT, and the one unavoidable copy is drift-guarded. |
| jsonwebtoken `use_pem` | disabled (`default-features = false`) | Unused (all keys DER). Removes the `pem`/`simple_asn1` parser surface. |
| Ed25519 verify semantics | aws-lc `ED25519_verify` (BoringSSL-derived) | Rejects S >= L, cofactorless, byte-compare of R. Equivalent to the 9.x ring path. Confirmed by @paired-security. |
| Key generation / PKCS#8 validation | unchanged: `ring` | ADR-0027 "ring for direct crypto". aws-lc-rs is reached only inside jsonwebtoken; no direct `aws_lc_rs::` call is added. |
| Algorithm allow-list | unchanged: `Validation::new(Algorithm::EdDSA)` at all three sites | 11.x also checks key family in the verifier constructors. HS* is unreachable from production validation. |
| rustls provider | `ring` only (push-gateway removed) | Exactly one rustls provider. Every TLS site already passes ring explicitly. |
| Provider invariant enforcement | `dt-guard resolved-features` (G2) over resolved features + aws-lc reverse-dependent backstop | Exactly-one-provider holds by guard, not by convention. Also enforces ADR-0027's "no direct aws-lc-rs". |
| Fuzz seed key | fixed `[0x42; 32]` seed in the harness (Lead-accepted) | Deterministic and reproducible; it is not a credential and verifies nothing outside the harness. |
| Fuzz target | EdDSA with a real public key + valid seed token generated at fuzz time (iteration 2; iteration 1 committed it) | The production verify path is now fuzzed. HS256 (ADR-0027-deprecated) is no longer exercised. |
| ADR-0027 | amend: new row, line 39 replacement, Signatures Notes scoped, review date | aws-lc-rs inside jsonwebtoken becomes the explicitly approved non-ring backend. Direct aws-lc-rs use still needs an amendment. |

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed (rev 3) |
| Test | confirmed (rev 3) |
| Observability | confirmed (rev 3 ack) |
| Code Quality | confirmed (rev 3) |
| DRY | confirmed (rev 3) |
| Operations | confirmed (rev 3) |
| Infrastructure (owner, dt-guard machinery) | confirmed (rev 3) |

Lead rulings at Gate 1: G1 (R3 features ⊇ root) and G2 (resolved-features one-provider guard) IN SCOPE; G2 wired in Layer 1 (compile.sh) per the layer3.sh cargo-zero invariant; fuzz key from a fixed public test seed (`[0x42;32]`); per-PR fuzz-workspace compile lane deferred into the existing docs/TODO.md entry (ADR-0033 design decision); observability `run_upkeep` pre-existing issue → docs/TODO.md. Operator ADR-0027 text used verbatim; paired-security flagged a non-blocking wording nit ("only approved non-ring backends" vs bcrypt/subtle) for the operator.

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | CLEAR | 0 | 0 | 0 | Independently re-ran aws-lc-rs reverse deps, rustls/webpki features, G2 adverse at 2869bf37, OpenSSL verify of seed; approved .gitguardian.yaml entry (entry removed in iteration 2); untrusted 0.7.1 acceptable |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | Layer-4 N/A fail-open evidence appended to existing TODO entry |
| Observability | CLEAR | 0 | 0 | 0 | No metrics endpoint change; run_upkeep pre-existing → TODO (Gate 1) |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | F1 evaluate split; F2 allow reason |
| DRY | CLEAR | 0 | 0 | 0 | Extraction opportunity appended to existing TODO duplication entry |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | Cold-registry false red → cargo-fetch-all-platforms lane |
| Infrastructure (owner) | RESOLVED-FIXED | 4 | 4 | 0 | Backstop precondition masking, relative --root, cargo diag text, policy list prose drift |

Semantic Guard: not spawned (no checks.md surface).

Lead fast-check note: two Lead layer-fast runs were disturbed by concurrent writers (run-story.test.sh containment abort; a run ending after L4 with no summary, shared /tmp/devloop logs overwritten). Isolated DEVLOOP_TMP runs green. Precondition lane, no attempt consumed.

### Gate 2 — Full Validation (Lead)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree (tree fingerprint identical before/after; all teammates frozen):

```
LAYER=1 RESULT=OK DURATION=8
LAYER=2 RESULT=OK DURATION=8
LAYER=3 RESULT=OK DURATION=63
LAYER=4 RESULT=OK DURATION=204
LAYER=5 RESULT=OK DURATION=3
LAYER=6 RESULT=OK DURATION=5
LAYER=7 RESULT=OK DURATION=1591
TOTAL_DURATION=1882 TOTAL_RESULT=OK
```
L7: `env-tests-passed`, `browser-e2e-passed`. L6: `cargo-audit-passed-preexisting-advisories`, `pnpm-audit-passed`. Attempts: L1-6 1/3 (two earlier Lead fast-check disruptions were concurrent-writer preconditions, not counted), L7 1/2.

---

## Pre-Work

None. The work starts from `2869bf37`, the dependabot bump with no other changes.

---

## Implementation Summary

### Crypto backend and provider invariant
| Item | Before | After |
|------|--------|-------|
| root `jsonwebtoken` | `"11.1"` (defaults = `use_pem`; no backend) | `{ version = "11.1", default-features = false, features = ["aws_lc_rs"] }` |
| fuzz `jsonwebtoken` | `version = "9.3"`, `default-features = false` | `version = "11.1"`, `default-features = false`, `features = ["aws_lc_rs"]` |
| root `metrics-exporter-prometheus` | `"0.18"` (defaults: `http-listener`, `push-gateway`) | `{ version = "0.18", default-features = false }`, with no features |
| rustls resolved provider features | `aws-lc-rs`, `aws_lc_rs`, `ring` | `ring` |
| aws-lc-rs reverse dependents | `rustls`, `rustls-webpki` | `jsonwebtoken` |

### Guards (infrastructure machinery; policy content security + auth-controller)
- **R3 (`workspace_deps.rs`)** now binds the excluded crate's `features` (it must be a superset of the root's) and its default-features (it must match the root in both directions). This uses the `default` pseudo-feature together with the existing root-off ⇒ excluded-off check.
- **`DepSpec` (`common/cargo_manifest.rs`)** now carries `features`. A malformed `features` or `default-features` value becomes `DepSpec::Malformed`, which is reported as a precondition and never as a pass.
- **G2 `dt-guard resolved-features`** is new and lives in `resolved_features/{mod,policy}.rs`. It is wired in Layer 1 (`compile.sh`) right after `service-dep-exclusion`.
- **`common/cargo_invoke.rs:run_cargo`** is the one place where dt-guard spawns cargo. `service_dep_exclusion` now uses it.

### Fuzz harness
- **Target.** `jwt_validation` now uses EdDSA with a real Ed25519 public key derived from the fixed `[0x42;32]` seed in `fuzz/common/jwt_seed_key.rs`. That file sits outside `fuzz_targets/`, because `crates/media-protocol/tests/fuzz_wiring.rs` requires every file there to be a `[[bin]]`.
- **Seed (iteration 2).** Not committed. `examples/gen_jwt_seed.rs` writes it at fuzz time to the gitignored `crates/ac-service/fuzz/corpus/jwt_validation/valid_eddsa` (cargo-fuzz's default corpus dir), reads it back and decodes it with the target's exact key and `Validation`, and fails otherwise. It logs the path, length and sha256, never the token. Iteration 1 committed it under `fuzz/seeds/` with a `.gitguardian.yaml` ignore; both are gone.
- **Workflow (iteration 2).** `fuzz-nightly.yml` runs each matrix entry's `seed_cmd` (jwt_validation: `gen_jwt_seed`; the 4 media-protocol targets: `corpus_seeds`) in a "Generate seed corpus" step before fuzzing. The fuzz step logs `corpus dir passed: fuzz/corpus/<target> (N pre-seeded inputs)` and fails with `::error::` if a target declaring `seed_cmd` has 0 inputs. The crash upload/notify steps fire only when the fuzz step itself failed.

### Additional Changes
- **`.dockerignore`: added `**/fuzz/`.** In iteration 1 `validate-docker-context` flagged the then-committed seed, which has the shape of a JWT and matches its `[content:JWT]` value-shape rule. It also flagged local fuzz corpora left by my 10k run. Iteration 2 keeps the rule: the scan walks untracked files, and the generated seed now lands in `fuzz/corpus/` locally, alongside corpora and crash artifacts. No Dockerfile reads `crates/*/fuzz` (`grep fuzz infra/docker` returns nothing), and these are excluded workspaces that no image builds. I didn't foresee this change in the plan; it is classified as infrastructure, Minor-judgment, and was raised to @team-lead.
- **Comments and docs.** The root `Cargo.toml` comments now give the mechanical facts and point to "Rationale: ADR-0027". ADR-0027 has the new row, the Signatures-row scope note, the line-39 sentence verbatim, an "Enforced by:" pointer, and Last reviewed set to 2026-11-05. `BUILD_REQUIREMENTS.md` was updated. The runbook §6.1 gained a `resolved-features-failed` row. TODO.md: the existing "Neither fuzz workspace is compiled" entry was extended rather than duplicated. Both INDEX files got pointers.

---

## Files Modified

```
 .dockerignore                                      |   5 +    (iteration 2 comment)
 .github/workflows/fuzz-nightly.yml                 |  71 +-   (iteration 2)
 Cargo.lock                                         |  37 +-
 Cargo.toml                                         |  26 +-
 crates/ac-service/fuzz/Cargo.toml                  |   9 +-
 crates/ac-service/fuzz/common/jwt_seed_key.rs      |  61 ++   (new)
 crates/ac-service/fuzz/examples/gen_jwt_seed.rs    |  92 ++   (new; iteration 2)
 .../ac-service/fuzz/fuzz_targets/jwt_validation.rs |  41 +-
 crates/dt-guard/src/common/cargo_invoke.rs         |  35 +    (new)
 crates/dt-guard/src/common/mod.rs                  |   1 +
 crates/dt-guard/src/common/cargo_manifest.rs       | 128 +++-
 crates/dt-guard/src/lib.rs                         |   1 +
 crates/dt-guard/src/main.rs                        |  15 +
 crates/dt-guard/src/resolved_features/mod.rs       | 771 +++  (new)
 crates/dt-guard/src/resolved_features/policy.rs    | 117 ++++ (new)
 crates/dt-guard/src/service_dep_exclusion.rs       |  18 +-
 crates/dt-guard/src/workspace_deps.rs              | 426 ++++++++++--
 crates/dt-guard/tests/resolved_features_e2e.rs     |  63 ++   (new)
 crates/dt-guard/tests/workspace_deps_e2e.rs        |  18 +-
 crates/media-protocol/tests/fuzz_wiring.rs         | 147 ++   (iteration 2)
 docs/BUILD_REQUIREMENTS.md                         |  25 +-
 docs/FUZZING.md                                    |  23 +    (iteration 2)
 docs/TODO.md                                       |   4 +    (incl. @observability's run_upkeep entry)
 docs/decisions/adr-0027-approved-crypto.md         |   9 +-
 docs/runbooks/devloop-validation.md                |   1 +
 docs/specialist-knowledge/auth-controller/INDEX.md |   1 +
 docs/specialist-knowledge/infrastructure/INDEX.md  |   2 +-
 scripts/lang/rust/compile.sh                       |  11 +
```

### Cargo.lock delta
- `jsonwebtoken` gains the `aws-lc-rs` dependency and drops `pem` and `simple_asn1`. `simple_asn1` leaves the lock; `pem` stays because something else still uses it.
- `metrics-exporter-prometheus` drops its `hyper`, `hyper-rustls`, `hyper-util`, `http-body-util`, `ipnet`, `rustls`, `tokio` and `tracing` edges.
- `rustls` and `rustls-webpki` drop `aws-lc-rs`.
- `hyper-rustls` drops `rustls-native-certs`.
- **New package: `untrusted 0.7.1`.** aws-lc-rs's default feature set pulls it in (`cargo tree -e features -i untrusted@0.7.1` shows `aws-lc-rs feature "alloc"` ← `"default"` ← jsonwebtoken). That set is on now because jsonwebtoken depends on aws-lc-rs with defaults, whereas rustls had pulled aws-lc-rs with defaults off. It is a small pure-Rust parsing crate, not a crypto backend. It does mean a second `untrusted` version, next to 0.9.0 used by ring. I flagged it to @paired-security.
- aws-lc-rs and aws-lc-sys each still appear at exactly one version (1.18.1 and 0.45.0). The `cmake` crate (0.1.54) was **already** in the lock before this change, as an unconditional build-dependency of aws-lc-sys. It is not new. The Linux build does not invoke it, because aws-lc-sys uses its prebuilt bindings with cc.

---

## Devloop Verification Steps

### Gate-3 evidence (real command output)

**Named controls.** All four runs below are quoted verbatim.

At `2869bf37`, running `cargo test -p common --lib test_verify_token_accepts_jwk_without_alg_field`:
```
test jwt::tests::test_verify_token_accepts_jwk_without_alg_field ... FAILED
thread 'jwt::tests::test_verify_token_accepts_jwk_without_alg_field' (38009) panicked at /tmp/cargo-home/registry/src/index.crates.io-1949cf8c6b5b557f/jsonwebtoken-11.1.0/src/crypto/mod.rs:125:42:
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 252 filtered out; finished in 0.03s
```
At `2869bf37`, running `cargo test -p ac-service --lib crypto::tests::test_jwt_sign_verify`:
```
test crypto::tests::test_jwt_sign_verify ... FAILED
thread 'crypto::tests::test_jwt_sign_verify' (38025) panicked at /tmp/cargo-home/registry/src/index.crates.io-1949cf8c6b5b557f/jsonwebtoken-11.1.0/src/crypto/mod.rs:124:40:
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 405 filtered out; finished in 0.05s
```
After the fix:
```
test jwt::tests::test_verify_token_accepts_jwk_without_alg_field ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 252 filtered out; finished in 0.00s
test crypto::tests::test_jwt_sign_verify ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 405 filtered out; finished in 0.00s
```
The full suites after the fix: `cargo test -p common --lib` gives `ok. 253 passed`. `cargo test -p ac-service --lib crypto` gives `ok. 67 passed`. Before the fix these were 8 and 19 failed.

**`cargo tree -e features -i rustls --workspace`** (rustls feature lines):
```
before:                          after:
rustls feature "aws-lc-rs"       rustls feature "ring"
rustls feature "aws_lc_rs"       rustls feature "std"
rustls feature "ring"            rustls feature "tls12"
rustls feature "std"
rustls feature "tls12"
```
`cargo tree -e features -i rustls --workspace | grep -ci aws` gives `0` after the fix.

**`cargo tree -i aws-lc-rs --workspace -e normal,build,dev --target all --depth 1`**:
```
before:                          after:
aws-lc-rs v1.18.1                aws-lc-rs v1.18.1
├── rustls v0.23.46              └── jsonwebtoken v11.1.0
└── rustls-webpki v0.103.16          [dev-dependencies]
```
In the "after" output, `[dev-dependencies]` is cargo's own label; it marks where jsonwebtoken is a dev-dep of some members. jsonwebtoken is still the only dependent.

**Lock** (`grep -A1 -E 'name = "(aws-lc-sys|aws-lc-rs|cmake)"' Cargo.lock`): the before and after output are identical, `aws-lc-rs 1.18.1`, `aws-lc-sys 0.45.0` and `cmake 0.1.54`. Each appears once, and `cmake` is pre-existing (see "Cargo.lock delta").

**G2 adverse pair.** The guard was built from this PR and run against the `2869bf37` manifests. Output is verbatim, apart from the trailing hint text, which I refined later:
```
SCOPE: 475 resolved nodes, 5 provider-policy crate instances, 2 reverse-dependent policies, 4 hits
VIOLATION: [provider-features-mismatch] jsonwebtoken@11.1.0 resolves provider features [], required exactly [`aws_lc_rs`]: missing [`aws_lc_rs`]. ...
VIOLATION: [provider-features-mismatch] rustls@0.23.46 resolves provider features [`aws-lc-rs`, `aws_lc_rs`, `ring`], required exactly [`ring`]: unexpected [`aws-lc-rs`, `aws_lc_rs`]. ...
VIOLATION: [provider-features-mismatch] rustls-webpki@0.103.16 resolves provider features [`aws-lc-rs`, `ring`], required exactly [`ring`]: unexpected [`aws-lc-rs`]. ...
VIOLATION: [reverse-dependent-disallowed] aws-lc-rs is depended on by [`rustls`, `rustls-webpki`]; allowed only [`jsonwebtoken`]. ...
STATUS=FAIL REASON=provider-features-mismatch-4
exit=1
```
After the manifest edits:
```
SCOPE: 475 resolved nodes, 5 provider-policy crate instances, 2 reverse-dependent policies, 0 hits
STATUS=OK REASON=resolved-features-clean-instances-5
exit=0
```

**R3 real-tree control.** `r3_real_fuzz_manifest_passes_and_fires_without_aws_lc_rs` and the e2e test `real_repo_is_clean_over_every_member` were both red on the `2869bf37` manifests, which still had the 9.3-vs-11.1 drift. Both are green after the fix. The unit test also checks that the real fuzz manifest with `"aws_lc_rs"` deleted produces exactly 1 R3 hit, `` `jsonwebtoken` is missing features [`aws_lc_rs`] ``.

**Fuzz: cargo-fuzz build, not a fallback.** I installed `cargo-fuzz 0.13.2` with `--locked` (the pin in `infra/cargo-tools.versions`) into a scratch `--root`. I ran `cargo +nightly fuzz build jwt_validation --verbose` from a clean `fuzz/target`:
- exit 0, finishing with `Finished release profile [optimized + debuginfo] target(s) in 43.06s`;
- 116 crates compiled;
- `Compiling aws-lc-sys v0.47.0`. The fuzz workspace has no lock, so it resolves fresh: 0.47.0 versus 0.45.0 in the root lock;
- the flags included `-Zsanitizer=address` and `sanitizer-coverage-level=4`, `-inline-8bit-counters`, `-pc-table`, `-trace-compares` and `-stack-depth`.

So **aws-lc-sys builds under the cargo-fuzz ASan and sancov flags.**

(Iteration-1 evidence; `fuzz/seeds/` no longer exists — iteration-2 re-proof below.) Seeded 10k run, `cargo +nightly fuzz run jwt_validation fuzz/corpus/jwt_validation fuzz/seeds/jwt_validation -- -runs=10000 -print_final_stats=1`:
```
INFO: seed corpus: files: 1 min: 172b max: 172b total: 172b rss: 44Mb
#2	INITED cov: 684 ft: 684 corp: 1/172b exec/s: 0 rss: 44Mb
#10000	DONE   cov: 1306 ft: 2284 corp: 211/24Kb lim: 172 exec/s: 0 rss: 64Mb
Done 10000 runs in 0 second(s)
stat::number_of_executed_units: 10000
exit=0
```
(Iteration-1 evidence; `fuzz/seeds/` no longer exists — iteration-2 re-proof below.) To get the zero-provider crash, I set `features = ["aws_lc_rs"]` to `features = []` in `fuzz/Cargo.toml`, ran `cargo +nightly fuzz run jwt_validation fuzz/seeds/jwt_validation -- -runs=1`, and then restored the file (`grep '^features'` afterwards shows `features = ["aws_lc_rs"]`):
```
thread '<unnamed>' (561722) panicked at /tmp/cargo-home/registry/src/index.crates.io-1949cf8c6b5b557f/jsonwebtoken-11.1.0/src/crypto/mod.rs:125:42:
Could not automatically determine the process-level CryptoProvider from jsonwebtoken crate features.
==561722== ERROR: libFuzzer: deadly signal
SUMMARY: libFuzzer: deadly signal
... Test unit written to .../fuzz/artifacts/jwt_validation/crash-65aadeee55491870d6c8a3f5a01662da96fbfd61
exit=1
```
The seed alone is what triggers it: input 1 reaches signature verification.

(Iteration-1 evidence; `fuzz/seeds/` no longer exists — iteration-2 re-proof below.) **Seed self-check and byte-identical regeneration.** `cargo run --example gen_jwt_seed` was run twice:
```
wrote seeds/jwt_validation/valid_eddsa (172 bytes); self-check: decode Ok, sub=fuzz-seed, alg=EdDSA
ed2334be3bfff3dbf9c0efbf6bceac334f1393a75b1e9d619c57baf69ff9d6ec  seeds/jwt_validation/valid_eddsa   (run 1)
ed2334be3bfff3dbf9c0efbf6bceac334f1393a75b1e9d619c57baf69ff9d6ec  seeds/jwt_validation/valid_eddsa   (run 2)
```
I also re-ran it after moving the key module to `fuzz/common/`; it gave the same sha256.

**`cargo check` in `crates/media-protocol/fuzz`**: `Finished dev profile ... in 3.90s`, which is clean. `cargo check --all-targets` in `crates/ac-service/fuzz` (targets plus example) is also clean.

### `./scripts/layer-fast.sh` (layers 1-6)
**Final run, after all Gate-3 fixes, with an isolated `DEVLOOP_TMP=<scratchpad>/devloop-tmp`**:
```
LAYER=1 RESULT=OK DURATION=6
LAYER=2 RESULT=OK DURATION=6
LAYER=3 RESULT=OK DURATION=65
LAYER=4 RESULT=OK DURATION=204
LAYER=5 RESULT=OK DURATION=2
LAYER=6 RESULT=OK DURATION=5
TOTAL_DURATION=288 TOTAL_RESULT=OK
STATUS=OK REASON=cargo-fetch-all-platforms-passed
STATUS=OK REASON=resolved-features-passed
     Summary [ 188.609s] 4763 tests run: 4763 passed, 0 skipped
```
Earlier runs are kept below.

**Status**: PASS (final run, after moving `run_cargo` into `common/cargo_invoke.rs` per the Lead)
```
LAYER=1 RESULT=OK DURATION=7
LAYER=2 RESULT=OK DURATION=6
LAYER=3 RESULT=OK DURATION=67
LAYER=4 RESULT=OK DURATION=182
LAYER=5 RESULT=OK DURATION=2
LAYER=6 RESULT=OK DURATION=8
TOTAL_DURATION=272 TOTAL_RESULT=OK
     Summary [ 163.828s] 4761 tests run: 4761 passed, 0 skipped
```
In the run before this one, the summary showed `LAYER=4 RESULT=N/A`, although nextest passed all 4761 tests and printed `layer4-summary` OK. At that point the 46-byte file `/tmp/devloop/layer-4.log` held only `STATUS=N/A REASON=not-applicable-to-this-lang`. I didn't confirm the cause. My best guess is another process writing the same shared path at the same time. I re-ran instead of explaining it away, and the re-run was clean.
- Layer 1 includes `STATUS=OK REASON=resolved-features-clean-instances-5`.
- Layer 6 audited 3 graphs. The root lock was `preexisting-advisories`, and `crates/ac-service/fuzz` and `crates/media-protocol/fuzz` (resolved fresh) were `ok`.
- Layer 7 is the Lead's to run.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Test Specialist
**Verdict**: RESOLVED-FIXED (pending re-check)
**Findings**: 1 found, 1 fixed, 0 deferred

- The `LAYER=4 RESULT=N/A` observation is now recorded as evidence in the existing `docs/TODO.md` entry for concurrent `layer-all.sh` runs sharing `${DEVLOOP_TMP}`. The note marks the attribution as unconfirmed, because no falsifier was captured, and states the fail-open consequence. No pipeline change was made.

### Observability Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Infrastructure Reviewer
**Verdict**: RESOLVED-FIXED (pending re-check)
**Findings**: 4 found, 4 fixed, 0 deferred

- **F1:** a backstop crate absent because a provider mismatch already fired is no longer reported as a precondition. It stays a precondition when no mismatch fired. A new unit test covers it: jsonwebtoken resolves `[]` and aws-lc-rs/aws-lc-sys are absent, giving exactly one `provider_features_mismatch`.
- **F2:** the manifest path is now `std::path::absolute(root/Cargo.toml)`. A new e2e test runs `--root ..` with the working directory set to `crates/`, and it passes.
- **F3:** `run_cargo` now sets `CARGO_TERM_COLOR=never` and drops the duplicated "exit" prefix. The line now reads `cargo metadata failed: exit status: 101: error: manifest path ... does not exist`.
- **F4:** the `main.rs` ResolvedFeatures doc and the `compile.sh` comment now just point at `resolved_features/policy.rs` without listing crates. The runbook row does the same and keeps the metrics-exporter triage example.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED (pending re-check by @code-reviewer)
**Findings**: 2 found, 2 fixed, 0 deferred

- **F1**: `resolved_features::evaluate` was about 140 lines. It is now split into `parse` (the whole-run preconditions), `check_provider_policy` and `check_reverse_dep`, which mirror `check_provider`, plus a `Graph` index that builds the `node_ids` `BTreeSet` once. That removes the O(packages × nodes) `any()`. Behaviour is unchanged: the 18 unit tests, 2 e2e tests and the real-tree `STATUS=OK` all still pass.
- **F2**: `encoding_key` now uses `#[allow(dead_code, reason = "...")]` with the reviewer's wording.
- **Side note (GitGuardian)** **[Superseded in iteration 2: the entry and the generator's assertion are removed; no token is committed.]**: `.gitguardian.yaml` gets an explicit `ignored_matches` entry for the seed JWT, with no path exclusion, following that file's own rule. The token now lives in two places, so `gen_jwt_seed` asserts that `.gitguardian.yaml` lists the token it writes. Checked against a copy with the entry removed: it exits 101 with "the seed token is not in .gitguardian.yaml". The fixed `SEED` is a `[0x42; 32]` byte-array literal, not a string a detector would match.

### DRY Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED

**True duplication findings** (entered fix-or-defer flow):
{List findings sent to implementer, or "None"}

**Extraction opportunities** (appended to `docs/TODO.md`):
{One bullet per `docs/TODO.md` entry added, citing the section heading the entry was added under, or "None"}

### Operations Reviewer
**Verdict**: RESOLVED-FIXED (pending re-check)
**Findings**: 1 found, 1 fixed, 0 deferred

- **The guard false-reds on a cold registry.** `cargo metadata` without `--filter-platform` needs every platform's sources, and the workspace build downloads only the host's. Fix (a): `compile.sh` now runs `run_and_emit "cargo-fetch-all-platforms" cargo fetch "${CARGO_LOCKED[@]}"` just before `resolved-features`. The comments in `compile.sh` and `mod.rs` were corrected. The runbook §6.1 gained a `cargo-fetch-all-platforms-failed` row, and the resolved-features precondition lane now names the cold-registry cause. Repro with a scratch `CARGO_HOME`:
```
$ CARGO_HOME=<scratch> cargo fetch --locked --target x86_64-unknown-linux-gnu
exit=0
$ CARGO_HOME=<scratch> cargo metadata --format-version 1 --offline --locked --manifest-path /work/Cargo.toml
error: failed to download `android_system_properties v0.1.5`
exit=101
$ CARGO_HOME=<scratch> cargo fetch --locked   (the new compile.sh step)
exit=0
$ CARGO_HOME=<scratch> dt-guard resolved-features --root /work
SCOPE: 475 resolved nodes, 5 provider-policy crate instances, 2 reverse-dependent policies, 0 hits
STATUS=OK REASON=resolved-features-clean-instances-5
exit=0
```

### Semantic Guard Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Native verdict**: SAFE / UNSAFE (mapped by Lead per `.claude/agents/semantic-guard.md` §Verdict Mapping)
**Findings**: {count} found, {count} fixed, {count} deferred

{Per-finding `[check-name]: file/path.rs:line - description` block, or "No findings"}

{Note any findings folded with Code Reviewer at Gate 3 §Deduplication, with "(also flagged by code-reviewer)" attribution.}

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

Examples:

```
- `docs/TODO.md` §Observability Debt — orphan recording-site audit follow-up
- `docs/TODO.md` §Cross-Service Duplication (DRY) — extract record_token_refresh_metrics
```

or:

```
- (none surfaced in this devloop)
```

Pointers for this devloop:
- `docs/TODO.md` §Media Path Obligations — per-PR compile lane for root-`exclude` fuzz workspaces (extended existing entry)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `2869bf37`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit. A partial revert is unsafe: reverting only the backend line re-breaks every JWT; reverting only the metrics-exporter line re-adds rustls's second provider and G2 reds; reverting only R3 or G2 leaves the backend selection unguarded. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Shared seed module inside `fuzz_targets/` broke the fuzz-wiring test
**Problem**: `crates/media-protocol/tests/fuzz_wiring.rs` requires every `fuzz_targets/*.rs` to be a `[[bin]]` with a nightly matrix entry. Putting `jwt_seed_key.rs` there failed Layer 4.
**Resolution**: I moved it to `crates/ac-service/fuzz/common/jwt_seed_key.rs` and recorded the reason in its doc comment. The seed's bytes did not change.

### Issue 2: `validate-docker-context` flagged the JWT-shaped seed and local corpora
**Problem**: the image build context admits all of `crates/`. The value-shape scan matched `[content:JWT]` on the seed (committed in iteration 1) and on local libFuzzer corpus files.
**Resolution**: I added `**/fuzz/` to `.dockerignore`, since no image reads the fuzz workspaces, and deleted the local corpora. In iteration 2 the seed is no longer committed, but the rule stays: the generated seed, corpora and crash artifacts still sit in untracked `crates/*/fuzz/` files, which the scan and the real build context both include.

### Issue 3: The scope guard needed one-path-per-row
**Problem**: a combined `lib.rs, main.rs` row and a `docs/TODO.md §...` row did not parse. A planned `release_build_profile.rs` row went unused, because nothing there destructures `DepSpec`.
**Resolution**: I split, merged and removed the rows. `validate-cross-boundary-scope` now reports clean.

### Issue 4: Clippy `-D clippy::panic` in new test code
**Resolution**: I replaced the `panic!`s with `assert!(matches!(..))` and `expect`.

---

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```

---

## Human Review (Iteration 2)

**Feedback**: "Generate the fuzz seed JWT at fuzz time and amend the commit." Rationale (operator): a committed, genuinely signed JWT will trip GitGuardian's public-repo monitoring, and ignoring security warnings is not acceptable. The commit `b415db50` is unpushed, so amending keeps the token out of public history entirely.

Scope for this iteration:
- Remove `crates/ac-service/fuzz/seeds/jwt_validation/valid_eddsa` and the `.gitguardian.yaml` `fuzz-seed-eddsa-jwt` entry; no JWT literal anywhere in the diff vs `2869bf37`.
- Generate the seed at fuzz time (fuzz-nightly + documented local command) via the existing deterministic `gen_jwt_seed` example; generation failure fails the fuzz job loudly.
- Keep the fuzz coverage properties: seed reaches aws-lc verify; zero-provider build crashes on it.

### Iteration 2 plan (after reviewer input and Lead rulings)

Invariant: **a fuzz target that has a seed generator is fuzzed seeded, every nightly, or the job fails.** No signed token is committed anywhere.

1. **Delete** `crates/ac-service/fuzz/seeds/` (only `jwt_validation/valid_eddsa`). **Restore** `.gitguardian.yaml` to its `2869bf37` state, which drops the `fuzz-seed-eddsa-jwt` entry; the file then leaves the diff.
2. **`examples/gen_jwt_seed.rs`** owns the seed path: `env!("CARGO_MANIFEST_DIR")` (the `crates/ac-service/fuzz` crate) joined with `OUT = "corpus/jwt_validation/valid_eddsa"`, so it lands in `crates/ac-service/fuzz/corpus/jwt_validation/` whatever the cwd (Lead correction). That is the gitignored directory cargo-fuzz reads by default, per the `docs/FUZZING.md` §Seed Corpus convention, so no caller passes a path (@dry-reviewer, Lead).
   - Takes **no arguments**. If any are given it prints usage to stderr and returns `ExitCode` 2.
   - Signs, writes, then **reads the file back** and decodes those bytes with the target's exact `decoding_key()`/`validation()`. It panics (non-zero) unless the result is `Ok` with `sub`/`alg` as expected.
   - Prints the resolved path, byte length, sha256 and self-check result. **Never the token.**
   - The `.gitguardian.yaml` assertion is removed.
3. **`fuzz-nightly.yml`**:
   - Each matrix entry that has a generator gets `seed_cmd:` on the line **after** `target:` (the `fuzz_wiring.rs` `windows(2)` constraint):
     - `jwt_validation`: `cargo +nightly run --manifest-path fuzz/Cargo.toml --example gen_jwt_seed`
     - the 4 `media-protocol` targets: `cargo +nightly test --locked -p media-protocol --test corpus_seeds` (`--locked` added at review, @media-handler F3; root `Cargo.lock` is tracked, the fuzz workspaces' locks are not, so `gen_jwt_seed` has none). Verified: `regenerate_fuzz_corpus` writes `fuzz/corpus/{codec_decode,codec_roundtrip,header_prefix,header_reader_split}/` relative to the crate, so this needs nothing beyond the matrix entry (Lead item 3).
     - `signaling_messages` has no generator, so no field.
   - New step **"Generate seed corpus"**:
     - `if: matrix.fuzz_target.seed_cmd` (truthy; an absent field is skipped), `working-directory: crates/<crate>`, `shell: bash`, `set -euo pipefail`.
     - The command comes in through `env: SEED_CMD` and runs as `read -ra cmd <<<"$SEED_CMD"; "${cmd[@]}"`, with no `${{ }}` in the script and no eval (@paired-security, @code-reviewer).
     - Working dir is `crates/<crate>`, not `crates/<crate>/fuzz`: media-protocol's generator is a root-workspace test, so one working dir serves both generators.
   - **"Run extended fuzz test"**:
     - Gets `id: fuzz`, `set -euo pipefail`, and the iteration-1 seeds/corpora array removed. The invocation is the original `cargo +nightly fuzz run "$TARGET"`, which uses the default `fuzz/corpus/<target>`.
     - Logs `corpus dir passed: fuzz/corpus/<target> (N pre-seeded inputs)`, a count only.
     - If `SEED_CMD` is non-empty and N == 0, it prints `::error::<target> declares seed_cmd but fuzz/corpus/<target> has 0 inputs` and exits 1. **[Moved at review (@observability): this check is now its own "Verify seed corpus" step before `fuzz`, so an empty seed corpus is never reported as a crash. See "Iteration 2 review fixes".]**
   - **"Upload crash artifacts"** and **"Notify on crash"** are gated on `failure() && steps.fuzz.outcome == 'failure'`, so a failed install or generation is not reported as crashes (@operations, Lead item 4).
4. **`crates/media-protocol/tests/fuzz_wiring.rs`** (@test A, Lead-accepted): closes the remaining "field deleted" silent-unseeded hole. Every `crates/<c>/fuzz/examples/*.rs` stem is named by `--example <stem>` in **exactly one** matrix entry's `seed_cmd` of crate `<c>`, and every `seed_cmd` `--example X` names an existing `crates/<c>/fuzz/examples/X.rs`. The existing parser still reads only `crate`/`target`, and the new `seed_cmd` parse is a third line. About a dozen lines.
5. **`.dockerignore`**: keep `**/fuzz/`. No image reads the fuzz workspaces, and the generated seed plus local corpora and crash artifacts land in `crates/*/fuzz/` inside the scanned build context (`docker_context.rs` walks untracked files). Only the comment is reworded.
6. **Docs.**
   - The generator header is the canonical description.
   - `docs/FUZZING.md` §Seed Corpus gets an ac-service `jwt_validation` entry with the exact working directory and two-command sequence.
   - `jwt_validation.rs` points at the generator and does not repeat the command.
   - `jwt_seed_key.rs` loses the "committed seed" wording.
   - The `docs/TODO.md` PR-101 EVIDENCE bullet now says the seed is "generated at fuzz time".
   - The workflow comments are updated.
   - main.md iteration-1 text: Cross-Boundary rows, 6b, Implementation Summary "Seed", Additional Changes, Issue 2, Files Modified and the Code Quality side note are made true. The iteration-1 evidence token text is redacted.
7. **Evidence** (real output, post-change tree):
   - (a) the documented local sequence end to end, `-runs=10000`, showing `seed corpus: files: 1` and cov above INITED;
   - (b) a zero-provider crash on the GENERATED seed, then restore and grep;
   - (c) negative controls: the run-step logic with `SEED_CMD` set and an empty corpus exits 1 with the `::error::`, and `gen_jwt_seed extra-arg` exits 2;
   - (d) `git status --short` clean after generation;
   - (e) the nightly generate command run verbatim for both generators;
   - (f) `cargo +nightly fuzz build jwt_validation`;
   - (g) the acceptance JWT grep over `git diff 2869bf37 -- .` is empty.

#### Files (iteration 2)
- `crates/ac-service/fuzz/seeds/jwt_validation/valid_eddsa` (deleted; leaves the diff)
- `.gitguardian.yaml` (restored to `2869bf37`; leaves the diff)
- `crates/ac-service/fuzz/examples/gen_jwt_seed.rs`
- `crates/ac-service/fuzz/fuzz_targets/jwt_validation.rs` (doc comment)
- `crates/ac-service/fuzz/common/jwt_seed_key.rs` (doc comment)
- `.github/workflows/fuzz-nightly.yml`
- `crates/media-protocol/tests/fuzz_wiring.rs`
- `.dockerignore` (comment)
- `docs/FUZZING.md`
- `docs/TODO.md` (evidence wording; scope-guard exempt)
- this main.md

#### Classification (new or changed rows)

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/ac-service/fuzz/examples/gen_jwt_seed.rs` (writes to gitignored `fuzz/corpus/jwt_validation/`, no args, read-back self-check, gitguardian check removed) | Not mine, Domain-judgment | security (paired; ADR-0027 crypto-primitive call site) |
| `crates/ac-service/fuzz/fuzz_targets/jwt_validation.rs` (doc comment only in iteration 2) | Not mine, Mechanical | security (paired) |
| `crates/ac-service/fuzz/common/jwt_seed_key.rs` (doc comment only in iteration 2) | Not mine, Mechanical | security (paired) |
| `.github/workflows/fuzz-nightly.yml` (per-target `seed_cmd` incl. media-protocol `corpus_seeds` entries, reviewed by protocol + media-handler; generate step, seeded-or-fail assertion, crash steps gated on `steps.fuzz`) | Not mine, Domain-judgment | infrastructure |
| `crates/media-protocol/tests/fuzz_wiring.rs` (every seed generator is wired to a `seed_cmd`, and every `seed_cmd` example exists; GSA `crates/media-protocol/**`: co-owners protocol + media-handler, security required; the guard admits exactly one Owner name, so protocol is listed; test reviews intent) | Not mine, Minor-judgment | protocol |
| `.dockerignore` (comment reworded, rule kept) | Not mine, Mechanical | infrastructure |
| `docs/FUZZING.md` (ac-service seed corpus local workflow) | Not mine, Minor-judgment | test |
| ~~`crates/ac-service/fuzz/seeds/jwt_validation/valid_eddsa`~~ (removed from the diff) | n/a | n/a |
| ~~`.gitguardian.yaml`~~ (removed from the diff) | n/a | n/a |

### Iteration 2 evidence (real output, post-change tree; no token text)

**(1) Generator contract.** Run from `crates/ac-service` (the documented cwd) and from the repo root. Both resolve to the same anchored path, with the same sha256 as iteration 1's committed file:
```
$ cargo +nightly run --manifest-path fuzz/Cargo.toml --example gen_jwt_seed
wrote /work/crates/ac-service/fuzz/corpus/jwt_validation/valid_eddsa (172 bytes, sha256 ed2334be3bfff3dbf9c0efbf6bceac334f1393a75b1e9d619c57baf69ff9d6ec); self-check: decode Ok, sub=fuzz-seed, alg=EdDSA
exit=0
$ cargo +nightly run -q --manifest-path fuzz/Cargo.toml --example gen_jwt_seed extra
usage: gen_jwt_seed (no arguments)
writes $CARGO_MANIFEST_DIR/corpus/jwt_validation/valid_eddsa (crates/ac-service/fuzz/corpus/jwt_validation/valid_eddsa)
exit=2
$ (repo root) cargo +nightly run -q --manifest-path crates/ac-service/fuzz/Cargo.toml --example gen_jwt_seed
wrote /work/crates/ac-service/fuzz/corpus/jwt_validation/valid_eddsa (172 bytes, sha256 ed2334be...d6ec); self-check: decode Ok, sub=fuzz-seed, alg=EdDSA
exit=0
```

**(2) Documented local sequence, seeded.** Run from `crates/ac-service` after `rm -rf fuzz/corpus fuzz/artifacts` and the generate command above. `cargo +nightly fuzz build jwt_validation` finished with exit 0 (`Finished release profile [optimized + debuginfo]`). Then `cargo +nightly fuzz run jwt_validation -- -runs=10000 -print_final_stats=1`:
```
INFO: seed corpus: files: 1 min: 172b max: 172b total: 172b rss: 44Mb
#2	INITED cov: 684 ft: 684 corp: 1/172b exec/s: 0 rss: 44Mb
#10000	DONE   cov: 1300 ft: 2279 corp: 195/20Kb lim: 172 exec/s: 0 rss: 64Mb
stat::number_of_executed_units: 10000
run exit=0
```
`git status --short` afterwards lists only the tracked edits of this change and no `??` lines. `--ignored` shows `!! crates/ac-service/fuzz/corpus/`, so the generated seed cannot be `git add -A`'d.

**(3) Zero-provider crash on the GENERATED seed.** Fresh corpus holding only the generated seed, with `fuzz/Cargo.toml` set to `features = []`, then `cargo +nightly fuzz run jwt_validation -- -runs=1`:
```
INFO: seed corpus: files: 1 min: 172b max: 172b total: 172b rss: 43Mb
thread '<unnamed>' panicked at .../jsonwebtoken-11.1.0/src/crypto/mod.rs:125:42:
Could not automatically determine the process-level CryptoProvider from jsonwebtoken crate features.
==1811549== ERROR: libFuzzer: deadly signal
exit=1
```
Restored afterwards: `grep '^features'` gives `features = ["aws_lc_rs"]`, `git diff -- crates/ac-service/fuzz/Cargo.toml` is empty, and the crash artifact was deleted.

**(4) Nightly steps run locally.** The `run: |` bodies were extracted verbatim from `fuzz-nightly.yml` and run with the step's `env:` and working directory. Where noted, `cargo` was an echo stub so that nothing fuzzes for 5.5 h.
```
## Generate seed corpus, ac-service (real cargo)
=== Generating seed corpus: cargo +nightly run --manifest-path fuzz/Cargo.toml --example gen_jwt_seed ===
wrote /work/crates/ac-service/fuzz/corpus/jwt_validation/valid_eddsa (172 bytes, sha256 ed2334be...d6ec); self-check: decode Ok, sub=fuzz-seed, alg=EdDSA
exit=0
## Generate seed corpus, media-protocol (real cargo; after rm -rf fuzz/corpus)
=== Generating seed corpus: cargo +nightly test --locked -p media-protocol --test corpus_seeds ===   (re-run with --locked after review)
test regenerate_fuzz_corpus ... ok
test result: ok. 4 passed; 0 failed; 0 ignored
exit=0
fuzz/corpus/codec_decode: 578 files
fuzz/corpus/codec_roundtrip: 578 files
fuzz/corpus/header_prefix: 578 files
fuzz/corpus/header_reader_split: 578 files
## (pre-review placement, check then in the run step; now in the generate step, see review fixes) Run step, NEGATIVE: jwt_validation, seed_cmd set, corpus dir absent
corpus dir passed: fuzz/corpus/jwt_validation (0 pre-seeded inputs)
::error::jwt_validation declares seed_cmd but fuzz/corpus/jwt_validation has 0 inputs; refusing to fuzz unseeded
exit=1
## Run step, NEGATIVE: same, corpus dir present but empty
::error::jwt_validation declares seed_cmd but fuzz/corpus/jwt_validation has 0 inputs; refusing to fuzz unseeded
exit=1
## Run step, positive: jwt_validation seeded (stub cargo)
corpus dir passed: fuzz/corpus/jwt_validation (1 pre-seeded inputs)
STUB cargo +nightly fuzz run jwt_validation -- -max_total_time=19800 -rss_limit_mb=4096 -workers=2 -print_final_stats=1
exit=0
## Run step, unaffected target: signaling_messages, no seed_cmd, no corpus dir (stub cargo)
corpus dir passed: fuzz/corpus/signaling_messages (0 pre-seeded inputs)
STUB cargo +nightly fuzz run signaling_messages -- ...
exit=0
```
The last case caught a bug in my first draft. It used `n=$(find "$corpus" -type f 2>/dev/null | wc -l)`, and under `set -euo pipefail` a missing corpus dir made `find` fail, which killed the step silently (exit=1, no message) for every target that has no corpus dir yet. The count is now guarded by `[ -d ]`.

The media-protocol nightly `cargo +nightly test` also prints nightly-only manifest lints (`missing [lints]` on the *-test-utils crates, "unused workspace dependency"). These are warnings, not failures, and they predate this change.

**(5) `fuzz_wiring.rs` new check, negatives.** Each was run against a temporarily edited workflow, which was restored afterwards (`cmp` confirms it); the check then passes again, 2 passed.
```
## seed_cmd line deleted from jwt_validation
ac-service/fuzz/examples/gen_jwt_seed.rs must be the seed_cmd (`--example gen_jwt_seed`) of exactly one fuzz-nightly matrix entry; found []
## example name typo (gen_jwt_sed)
ac-service/jwt_validation: seed_cmd names --example gen_jwt_sed, but .../crates/ac-service/fuzz/examples/gen_jwt_sed.rs does not exist
## same example on two entries
... of exactly one fuzz-nightly matrix entry; found ["jwt_validation", "signaling_messages"]
```

**(6) Acceptance grep.** `git diff 2869bf37 -- . | grep -cE 'eyJ[A-Za-z0-9_-]+\.eyJ'` returns 0; see the layer-fast report below for the final run. `git diff 2869bf37 -- .gitguardian.yaml` is empty, and `crates/ac-service/fuzz/seeds/` no longer exists.

### Iteration 2 review fixes

**F1 (@protocol, @media-handler; Lead ruling = @protocol's generic rule + @media-handler's anchor).** `every_fuzz_seed_example_is_run_by_exactly_one_nightly_entry` now also requires:
- every matrix entry of a crate that has a crate-wide `seed_cmd` (one naming no `--example`) to declare that identical `seed_cmd`;
- any crate with `tests/corpus_seeds.rs` to have at least one `seed_cmd` running `--test corpus_seeds`. Without this, the rule above passes vacuously when every entry's `seed_cmd` is deleted.

Adverse checks, run against a temporarily edited workflow; restored after each (`cmp` confirms), and the check then passes again (2 passed):
```
## one media-protocol seed_cmd deleted (header_prefix)
assertion `left == right` failed: media-protocol/header_prefix: media-protocol has the crate-wide seed_cmd `cargo +nightly test --locked -p media-protocol --test corpus_seeds`, so every media-protocol matrix entry must declare it
  left: None
 right: Some("cargo +nightly test --locked -p media-protocol --test corpus_seeds")
test result: FAILED. 1 passed; 1 failed
## all four media-protocol seed_cmds deleted
media-protocol has tests/corpus_seeds.rs (a seed generator) but no fuzz-nightly seed_cmd runs `--test corpus_seeds`
test result: FAILED. 1 passed; 1 failed
```

**F3 (@media-handler nit, Lead: apply where a lock exists).** `--locked` was added to the 4 media-protocol `seed_cmd`s (root workspace, tracked `Cargo.lock`). It was not added to `gen_jwt_seed`: `crates/ac-service/fuzz/Cargo.lock` is untracked (`.gitignore` `**/fuzz/Cargo.lock`), so `--locked` cannot hold there. The verbatim generate step with `--locked` gave exit 0, `test result: ok. 4 passed`, and 578 files in each of the 4 corpus dirs.

**F2 (per-target upload path).** Skipped per the Lead's ruling.

**Observability finding: an empty seed corpus was reported as a crash.** The seeded-or-fail check exited inside the `id: fuzz` step. That made `steps.fuzz.outcome == 'failure'`, so "Upload crash artifacts" and "Notify on crash" fired with no fuzzing having run. **Final placement (Lead ruling, @test F-T1 is the same finding): a separate "Verify seed corpus" step** between "Generate seed corpus" and `fuzz`, with the same `if: matrix.fuzz_target.seed_cmd`. An interim version appended the check to the generate step; the first re-run below is from that version, and the final re-run follows it. The `fuzz` step keeps only the `corpus dir passed: ... (N pre-seeded inputs)` log and no longer reads `SEED_CMD`. Re-run of the step bodies, extracted verbatim from the workflow:
```
## NEGATIVE: seed_cmd ran (no-op `true`) but wrote nothing
=== Generating seed corpus: true ===
::error::jwt_validation declares seed_cmd but fuzz/corpus/jwt_validation has 0 inputs after it ran; refusing to fuzz unseeded
exit=1
## positive: real seed_cmd (ac-service)
wrote /work/crates/ac-service/fuzz/corpus/jwt_validation/valid_eddsa (172 bytes, sha256 ed2334be...d6ec); self-check: decode Ok, sub=fuzz-seed, alg=EdDSA
seed corpus: fuzz/corpus/jwt_validation (1 inputs)
exit=0
## positive: media-protocol (TARGET=header_prefix)
test result: ok. 4 passed; 0 failed
seed corpus: fuzz/corpus/header_prefix (578 inputs)
exit=0
## fuzz step, stub cargo: jwt_validation logs "(1 pre-seeded inputs)" exit=0; signaling_messages logs "(0 pre-seeded inputs)" exit=0
```

**Final re-run, separate "Verify seed corpus" step.** The three step bodies were extracted verbatim and run in job order. A failing step stops the job, as GitHub does by default.
```
## NEGATIVE: Generate (no-op seed_cmd `true`) -> Verify -> fuzz
=== Generating seed corpus: true ===
generate exit=0
::error::jwt_validation declares seed_cmd but fuzz/corpus/jwt_validation has 0 inputs after its seed_cmd ran; refusing to fuzz unseeded
verify exit=1
fuzz step NOT reached
## POSITIVE (jwt_validation, real generator, stub cargo for the fuzz step)
wrote /work/crates/ac-service/fuzz/corpus/jwt_validation/valid_eddsa (172 bytes, sha256 ed2334be...d6ec); self-check: decode Ok, sub=fuzz-seed, alg=EdDSA
seed corpus: fuzz/corpus/jwt_validation (1 inputs)
corpus dir passed: fuzz/corpus/jwt_validation (1 pre-seeded inputs)
STUB cargo +nightly fuzz run jwt_validation -- -max_total_time=19800 ...
exit=0
## signaling_messages (no seed_cmd: Generate/Verify skipped by `if:`), stub cargo
corpus dir passed: fuzz/corpus/signaling_messages (0 pre-seeded inputs)
signaling_messages exit=0
```
Why the crash steps don't fire: the `fuzz` step is never reached after Verify fails, so its outcome is `skipped`. "Upload crash artifacts" and "Notify on crash" are gated on `failure() && steps.fuzz.outcome == 'failure'`, which is false. "Upload corpus" is `success()`, which is also false. That last part is read off the expressions; it was not run on a GitHub runner.

**@code-reviewer F1: the wiring test is split into one test per rule.** Each test keeps its non-vacuity guard: `seed_cmds()` asserts it found at least one entry, and the corpus_seeds and examples tests each assert `checked > 0`. With the workflow restored, all 5 pass. Adverse re-runs, restoring after each:
```
## one media-protocol seed_cmd deleted   -> FAILED: crate_wide_seed_cmd_is_declared_by_every_entry_of_its_crate
## all four deleted                      -> FAILED: every_corpus_seeds_test_is_run_by_a_seed_cmd
## gen_jwt_seed field deleted            -> FAILED: every_fuzz_seed_example_is_run_by_exactly_one_nightly_entry
## example typo (gen_jwt_sed)            -> FAILED: every_seed_cmd_example_exists (+ the exactly-one test)
restored                                 -> test result: ok. 5 passed
```
**@code-reviewer F2**: the plan's `if:` is now the truthy form the workflow uses, and the Files Modified counts are updated to the current diff vs `2869bf37`. **@operations nit**: the iteration-1 6b bullets are tagged "(iteration 1; superseded)".

### Iteration 2 — Gate 1

All 7 standing reviewers confirmed the plan. GSA owners @protocol and @media-handler (for `crates/media-protocol/tests/fuzz_wiring.rs`) were NOT on the panel at Gate 1. That was a Lead miss: they were pulled in after implementation and confirmed the plan retroactively. Recorded as a process deviation from ADR-0024 §6.4 (owner confirmation at Gate 1).

Lead step: `git reset --soft 2869bf37` (non-destructive; `b415db50` was unpushed and is kept in the reflog), so the scope guard compares the full amended change rather than the iteration-2 delta.

### Iteration 2 — Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | CLEAR | 0 | 0 | 0 | Flagged the stale staged index (Lead step; fixed by `git add -A`); accepted residuals: corpus artifact contains generated token, crash-log echo |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | F-T1: unseeded refusal misreported as crash → separate Verify step |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | Same as F-T1 |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | F1 split fuzz_wiring test into 4; F2 main.md drift |
| DRY | CLEAR | 0 | 0 | 0 | Cosmetic FUZZING.md paraphrase nit, no action |
| Operations | CLEAR | 0 | 0 | 0 | Ran FUZZING.md local sequence verbatim; superseded-tag nits applied |
| Infrastructure (owner) | RESOLVED-FIXED | 1 | 1 | 0 | Staged-index blocker (Lead step) |
| Protocol (GSA owner) | RESOLVED-FIXED | 1 | 1 | 0 | F1: crate-wide seed_cmd rule |
| Media-handler (GSA co-owner) | RESOLVED-FIXED | 1 | 1 | 0 | All-deleted corpus_seeds anchor; `--locked` on media-protocol seed_cmds |

### Iteration 2 — Gate 2 (Lead)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree (HEAD `2869bf37` after the soft reset; tree fingerprint identical before and after; all 10 teammates frozen):

```
LAYER=1 RESULT=OK DURATION=7
LAYER=2 RESULT=OK DURATION=6
LAYER=3 RESULT=OK DURATION=70
LAYER=4 RESULT=OK DURATION=162
LAYER=5 RESULT=OK DURATION=1
LAYER=6 RESULT=OK DURATION=6
LAYER=7 RESULT=OK DURATION=1328
TOTAL_DURATION=1580 TOTAL_RESULT=OK
```
L7: `env-tests-passed`, `browser-e2e-passed`. Committed as a single replacement for the unpushed `b415db50` (equivalent to `--amend`; the old commit stays only in the local reflog).
