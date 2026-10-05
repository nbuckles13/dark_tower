# Devloop Output: sqlx 0.8 → 0.9 workspace upgrade

**Date**: 2026-10-05
**Task**: Upgrade sqlx 0.8 → 0.9 across the workspace (supersedes Dependabot #65); step 3 of 3 on the consolidated dependency-upgrade branch
**Specialist**: database
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/3-sqlx-0-9`
**Duration**: ~2h10m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5d04dfb59b39915062828b3e02278ba0b2ef600d` |
| Branch | `feature/3-sqlx-0-9` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@session-1917ea97` |
| Implementing Specialist | `database` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security@session-1917ea97` |
| Test | `test@session-1917ea97` |
| Observability | `observability@session-1917ea97` |
| Code Quality | `code-reviewer@session-1917ea97` |
| DRY | `dry-reviewer@session-1917ea97` |
| Operations | `operations@session-1917ea97` |
| Semantic Guard | `semantic-guard@session-1917ea97` |

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
Upgrade the workspace `sqlx` 0.8.6 → 0.9.0 (latest 0.9.x; supersedes Dependabot #65, which failed to compile `gc-service`), keeping `migrations/` byte-identical in effect and every sqlx-cli consumer on the same version.

### Scope
- **Service(s)**: ac-service, gc-service (+ ac-test-utils, gc-test-utils via the workspace dep). Only gc-service needs a source edit.
- **Schema**: No. `migrations/**` untouched; `_sqlx_migrations` shape + checksums verified identical (Planning §3).
- **Cross-cutting**: Yes — Cargo.lock, the audit-suppression manifest (the bump removes `rsa` from the lockfile), and the devloop image's sqlx-cli (host rebuild).

### Debate Decision
NOT NEEDED — dependency upgrade with no design change; one compile-forced API adaptation.

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
| `Cargo.toml` | Mine | — |
| `Cargo.lock` (regen) | Mine | — |
| `crates/gc-service/src/handlers/meetings.rs` | Not mine, Minor-judgment | global-controller |
| `.sqlx/sqlx-data.json` (cleanup) | Mine | — |
| `.dockerignore` | Not mine, Mechanical | infrastructure |
| `audit-suppressions.toml` | Not mine, Minor-judgment | security |
| `.cargo/audit.toml` (regen) | Not mine, Mechanical | infrastructure |
| `scripts/lang/rust/audit.sh` | Not mine, Minor-judgment | infrastructure |
| `scripts/lang/_audit_gate.test.sh` | Not mine, Minor-judgment | infrastructure |
| `docs/TODO.md` | Not mine, Minor-judgment | security |
| `docs/contributor/audit-suppressions.md` | Not mine, Mechanical | infrastructure |
| `docs/runbooks/devloop-validation.md` | Not mine, Mechanical | operations |
| `crates/ac-service/Cargo.toml` | Mine | — |
| `crates/ac-service/src/main.rs` (call-site replacement) | Not mine, Minor-judgment | auth-controller |
| `crates/gc-service/src/main.rs` (call-site replacement) | Not mine, Minor-judgment | global-controller |
| `crates/gc-service/Cargo.toml` | Not mine, Mechanical | global-controller |
| `crates/common/Cargo.toml` | Not mine, Minor-judgment | dry-reviewer + code-reviewer (shared crate) |
| `crates/common/src/lib.rs` | Not mine, Mechanical | dry-reviewer + code-reviewer (shared crate) |
| `crates/common/src/db.rs` (new) | Mine | — |
| `docs/runbooks/ac-service-deployment.md` | Not mine, Mechanical | operations |
| `docs/runbooks/gc-deployment.md` | Not mine, Mechanical | operations |

**Deliberately NOT touched** (prose, not rows — see `docs/TODO.md` on the table's parser):
- `migrations/**` (GSA) — no change; equivalence proven, Planning §3.
- sqlx-cli consumers — `infra/lib/cargo-lock-version.sh`, `infra/kind/scripts/deploy.sh`, `infra/docker/db-migrate/Dockerfile`, `infra/devloop/devloop.sh`, `infra/devloop/Dockerfile`, `.github/workflows/ci.yml`, `scripts/lang/rust/test.sh`, `scripts/lang/rust/behavior-equivalence.test.sh`: all derive the version from Cargo.lock via `cargo_lock_version`; none hardcodes 0.8. `scripts/setup.test.sh` uses synthetic fixture lockfiles (`0.8.6`/`0.7.4` are fixture values, not pins).
- `.pnpm-audit-ignore.json` — regenerated by `--fix` but byte-identical (already `ignore: []`).
- `scripts/audit-suppressions-check.sh:411` — a historical code comment naming the ID; accurate as history.
- No `sqlx.toml` and no `sqlx-toml` feature (deliberate non-change, @operations): the library (not a default) and the CLI (`--no-default-features`) both run without it, so both use the default `_sqlx_migrations` table name and empty `ignored_chars`. If only one side enabled it, a `table_name`/`ignored_chars` setting could make them silently disagree.
- `crates/ac-service/tests/README.md`, `crates/ac-service/tests/fault_injection/slow_query_notes.md` — checked; their `#[sqlx::test]` descriptions (isolated per-test DBs, no hooks for injecting `pg_sleep`) are still accurate under 0.9.

---

## Planning

All evidence below is from read-only investigation: the 0.9.0 CHANGELOG/source in the cargo registry, and a trial upgrade in a scratch copy of the tree (own `CARGO_TARGET_DIR`; no tracked file edited).

**Mechanism restatement.** Instance: "bump sqlx". Mechanism: "every artifact that encodes the sqlx version, or a fact that was true *because of* the sqlx version, must move together." That is wider than the task names in one place: the `RUSTSEC-2023-0071` suppression exists *only* because sqlx 0.8 recorded `sqlx-mysql → rsa` in the lockfile. 0.9 gates `rsa` behind the new `mysql-rsa` feature, so `rsa` leaves Cargo.lock and the suppression becomes dead. Its own text names this as "the only real exit". Completing the invariant = remove it (security-owned content).

### 1. Version and features
- Latest 0.9.x is **0.9.0** (2026-05-06). MSRV 1.94; workspace/CI/Dockerfiles use 1.95.
- All workspace features (`runtime-tokio`, `postgres`, `uuid`, `chrono`, `migrate`) still exist under the same names. Only removed features are the combined `runtime-*-tls` ones, which we don't use. Change: `Cargo.toml:90` `"0.8"` → `"0.9"`; `cargo update -p sqlx`.
- Lockfile after: all 7 `sqlx*` packages are at 0.9.0, one version each. **`cargo tree -d | grep sqlx` gives a false positive**: it lists `sqlx-postgres`/`sqlx-core` as *dependents* of other duplicated crates (base64, hmac…). The precise check is `grep -A1 'name = "sqlx' Cargo.lock`, which shows exactly one version per sqlx crate. I'll record both.
- Duplicate count goes from 30 to 32. New: `hmac` 0.12/0.13, `syn` 2/3 (thiserror-impl 2.0.21, futures-macro), `r-efi` 5/6. Resolved: `const-oid`. None is a sqlx duplicate.
  - **`hmac` 0.12 → remove (Lead note).** `crates/ac-service/Cargo.toml` declares `sha2 = "0.10"` and `hmac = "0.12"` under "For field hashing (ADR-0011 privacy)", but nothing uses either crate. The field hashing is `ring::hmac` (`src/observability/mod.rs:36,60-61`), and there is no `hmac::Mac`/`Hmac<`/`sha2::` anywhere in ac-service or ac-test-utils. I'll delete both lines and that comment. Trialed: ac-service + ac-test-utils `--all-targets` check clean; the lockfile then has only `hmac 0.13.0`. The `sha2` 0.10.9/0.11.0 split remains, but both versions come from inside sqlx 0.9 (sqlx-core/sqlx-macros-core → 0.10; sqlx-postgres, wtransport → 0.11), so nothing in-tree can resolve it. Accepted.
  - **`sha2` 0.10/0.11 provenance (trial `cargo tree -i`):** `sha2 0.10.9` ← `sqlx-core 0.9.0`, `sqlx-macros-core 0.9.0`. `sha2 0.11.0` ← `sqlx-postgres 0.9.0`, `wtransport 0.7.1`. sqlx 0.9 itself carries both. Accepted.
  - **`syn` 2/3: accepted.** No workspace crate depends on `syn`. syn 3 enters only through `thiserror-impl 2.0.21` (`syn = "3"`) and `futures-macro 0.3.34` (`syn = "3.0"`). Both bumps are forced by sqlx 0.9's minimums (`thiserror = "2.0.18"`, `futures-util/-core = "0.3.32"`; the lock had 2.0.17/0.3.31). syn 2 stays for every other proc-macro (clap_derive, async-trait, asn1-rs-derive, …). No workspace pin can collapse it: holding thiserror/futures back would be a hold-back, and they are transitive.
  - **`r-efi` 5/6: accepted.** `r-efi 6.0.0` ← `getrandom 0.4.3` ← `rand 0.10.3` ← `sqlx-postgres 0.9.0` only. `r-efi 5.3.0` ← `getrandom 0.3.4` (quinn-proto, governor, rand_core 0.9, …). The workspace pins `rand = "0.8"` and has no direct getrandom/r-efi, so no in-tree pin collapses it.
  - Full lockfile delta (trial): sqlx* 0.8.6→0.9.0; futures* 0.3.31→0.3.34; thiserror(-impl) 2.0.17→2.0.21; flume 0.11→0.12; hashlink 0.10→0.11; etcetera 0.8→0.11; whoami 1.6→2.1; hkdf/md-5/sha1 → RustCrypto 0.13/0.11/0.11; added hmac 0.13, rand 0.10, getrandom 0.4, r-efi 6, syn 3, chacha20, cmov, ctutils. Removed: rsa and its pkcs/der/spki/signature/num-bigint-dig chain, home, libredox, wasite, windows-* 0.48.

### 2. Breaking changes read (CHANGELOG 0.9.0 §Breaking) → impact here
| Change | Impact |
|---|---|
| #3723 `SqlSafeStr`: `query*()` takes only `&'static str` or `AssertSqlSafe` | **2 call sites**, `gc-service/src/handlers/meetings.rs:802,815` (`format!("{} WHERE …", MEETING_SELECT_QUERY)`). This is exactly what broke Dependabot #65. Fix below. |
| `query!`-family macros (#3541 nullability etc.) | None in use. Verified: no `query!`/`query_as!`/`query_scalar!`/`migrate!` anywhere; only runtime `query`/`query_as`/`query_scalar`, `FromRow`, `#[sqlx::test]`. |
| `Migrate` trait changes, `sqlx.toml` | We don't implement `Migrate`. `sqlx-toml` is off in both the library (not a default) and our CLI build (`--no-default-features`), so the default table name and checksum apply. |
| #3800 `PgConnectOptions::options()` now escapes | **Hit after §12.** `add_query_timeout` (URL `options=` concat) is deleted, and `common::db::connect_pool_with` now sets the timeout via `.options([("statement_timeout", "<N>ms")])`, which is exactly the escaped path. The value contains no space or backslash, so escaping doesn't change it. Covered by `common::db` tests `connect_pool_applies_statement_timeout_via_the_url_path`, `connect_pool_overrides_a_statement_timeout_in_the_url` and `connect_pool_with_overrides_a_statement_timeout_set_via_options` (`SHOW statement_timeout` = `5s`, each override test with a 45s control). (The earlier Gate-1 probe of the URL `options=` path stays valid for operator-supplied URL options.) |
| #3993 pgpass unescape, #4201 SCRAM SASLprep fix, whoami v2 (`PGUSER` fallback) | Behavioral credential-path changes. See @security note below. |
| #3486 tracing field `aquired_after_secs` spelling | Nothing parses sqlx tracing fields (no `sqlx` targets in filters/dashboards). |
| #3952 `Pool::close` waits for all connections | Services never call `close()`. AC fault-injection tests do, and they pass. |
| #3613/#3674/#3924/#3928/#4008/#4142 etc. | RawSql, Cow decode, MySQL/SQLite, derive(Type) arrays: none used. |
| `#[sqlx::test]` harness | `_sqlx_test.databases` DDL is identical between 0.8.6 and 0.9.0. Only the futures plumbing changed. |

### 3. Migrations: proven identical (no schema change)
- Source: the `_sqlx_migrations` DDL is identical apart from the now-configurable `{table_name}`, which defaults to `_sqlx_migrations`. The checksum is still SHA-384 of the file. `ignored_chars` defaults to empty, so the hash is unchanged.
- Empirical: two fresh DBs on the devloop Postgres, one migrated by sqlx-cli **0.8.6** and one by **0.9.0** (`--source migrations`). All 10 `_sqlx_migrations` rows (version, description, checksum hex, success) are identical. A catalog dump of public columns, constraints, indexes, function bodies (md5) and triggers is identical (244 lines). `pg_dump` couldn't be used: client 15 vs server 16. Running the **0.9 CLI over the 0.8-migrated DB** is a clean no-op (rc 0, no checksum complaint). That is the upgrade path for existing dev clusters. Scratch DBs were dropped afterward.

### 4. sqlx-cli consumers
- `cargo install sqlx-cli --locked --no-default-features --features postgres --version =0.9.0` **works**, actually run into a scratch `--root` (22 s). The CHANGELOG warns "`cargo install --locked sqlx-cli` will no longer work" because upstream stopped tracking Cargo.lock in git. The published 0.9.0 `.crate` still ships a `Cargo.lock`, though, so `--locked` resolves. Same flags, same feature names (`postgres` is unchanged; `native-tls`/`sqlx-toml` stay off as before).
- `sqlx --version` prints `sqlx-cli 0.9.0`, so `test.sh`'s `awk '{print $2}'` parse still holds. The db-migrate ENTRYPOINT flags (`migrate run --source --connect-timeout`) all still exist.
- Every consumer derives the version via `infra/lib/cargo-lock-version.sh`, so after the Cargo.lock bump they pick up 0.9.0 with no edits. Grep found no hardcoded `0.8` pin.
- **In-container sqlx-cli is still 0.8.6.** `scripts/lang/rust/test.sh` will (correctly) fail Layer 4/5 with `SQLX_CLI_VERSION_MISMATCH` until the host runs `./infra/devloop/devloop.sh --rebuild --recreate`. For my own `layer-fast.sh` runs I'll put the scratch-installed 0.9.0 CLI first on `PATH` (`PATH=<scratch>/sqlx-cli-root/bin:$PATH`). That supplies the *correct* CLI and doesn't bypass the check, and I'll record it in main.md. The Lead's Gate-2 `layer-all.sh` needs either the rebuilt image or the same PATH prefix. The kind db-migrate image rebuilds itself on deploy (`deploy.sh` reads Cargo.lock).

### 5. `.sqlx/sqlx-data.json` is a dead legacy artifact → propose deleting it
Evidence: (a) it is the ≤0.6 single-file offline format (`{"db":"PostgreSQL"}`, zero queries), committed once in `61cbe660` (2025-11-22) and never touched since; (b) sqlx ≥0.7, including 0.9's `sqlx-macros-core/src/query/{mod,data}.rs`, reads only `.sqlx/query-<hash>.json` and never `sqlx-data.json`; (c) the workspace has no `query!`-family macros, so offline mode has nothing to serve; (d) `SQLX_OFFLINE` isn't set anywhere (CI, Dockerfiles, scripts, `.cargo/config.toml`); (e) earlier devloops (2026-09-27 mc-server-mute-teardown, 2026-08-14) already described it as "an empty stub". Plan: delete `.sqlx/`, and drop `.sqlx/` from the "must stay" list in the `.dockerignore` header comment. ADR-0024 / devloop SKILL's "`.sqlx/` offline data freshness" check for `migrations/` diffs stays: it remains the right rule if `query!` is ever adopted.

### 6. Code change: `gc-service/src/handlers/meetings.rs`
`MEETING_SELECT_QUERY` becomes `macro_rules! meeting_select_query { () => { r#"…"# } }`, and the two call sites become `sqlx::query(concat!(meeting_select_query!(), " WHERE meeting_code = $1"))` / `…meeting_id = $1`. This yields a `&'static str` that is byte-identical to the old `format!` output. I chose it over `AssertSqlSafe(format!(…))` because the SQL is fully static, so it stays inside the compile-time guarantee rather than adding an escape hatch for reviewers to audit. The macro stays file-local (no `#[macro_export]`), defined above its first use where the const was, with a short `//` comment explaining why it's a macro and not a const: `concat!` needs literal tokens, and sqlx 0.9's `query()` takes `SqlSafeStr` (`&'static str`), so a const + `format!` no longer compiles (@code-reviewer). Trial: `cargo check`/`clippy --all-targets -D warnings` clean.

### 7. RUSTSEC-2023-0071 suppression: remove (security content)
After the bump, `rsa` is absent from Cargo.lock, and the trial `cargo audit` with **no** `.cargo/audit.toml` reports no vulnerabilities (only the pre-existing allowed warnings). Plan: delete the entry + rationale block from `audit-suppressions.toml`, then `scripts/audit-suppressions-check.sh --fix`. Trialed with an empty manifest: `.cargo/audit.toml` → `ignore = []`, check `STATUS=OK REASON=suppressions-clean`. Stale references: `docs/TODO.md` — mark the RUSTSEC-2023-0071 entry (~L2055), the Suppressed Advisories table row (~L2347), Cluster C (~L2423/2431) and the `sqlx-mysql 0.8.6` sub-bullet (~L2483) resolved, citing this devloop; `docs/contributor/audit-suppressions.md:76` and `docs/runbooks/devloop-validation.md:530` use it as the worked example of a verify command, so generalise them to "the verify command of record in the entry's comment". Regression safety: if `rsa` ever re-enters the lockfile, Layer-6 `cargo audit` fails loudly. No new guard needed.

### 8. Validation done in trial
`cargo check`/`clippy --workspace --all-targets -D warnings` are clean. `cargo test --workspace --no-fail-fast` against the devloop DB: 4899 passed, 5 failed. All 5 are dt-guard real-tree tests that panic with "could not locate repo root (no .git ancestor)", an artifact of the scratch copy having no `.git`; they don't touch sqlx. All AC/GC `#[sqlx::test]` suites passed.

### 9. `#[sqlx::test]` runtime flavor is still current-thread (@test item 1)
Every `#[sqlx::test]` body runs through `sqlx_core::rt::test_block_on`: `sqlx-macros-core-0.9.0/src/test_attr.rs:73` for the simple form, and for `migrations = …` the `TestFn::run_test` → `sqlx-core-0.9.0/src/testing/mod.rs:224` `crate::rt::test_block_on(async move {…})`. In `sqlx-core-0.9.0/src/rt/mod.rs:147-160` the tokio arm is `tokio::runtime::Builder::new_current_thread().enable_all().build()…block_on(f)`, the same as 0.8.6 `rt/mod.rs:116-123`. One subtlety is new in 0.9: the `cfg_if!` checks `_rt-async-io` *before* `_rt-tokio`. The resolved `sqlx-core` feature set under 0.9 is `_rt-tokio, any, chrono, crc, default, json, migrate, offline, serde, serde_json, sha2, tokio, tokio-stream, uuid`, with no `_rt-async-io`, and it is identical to 0.8.6's (`cargo tree -e features -i sqlx-core`, compared side by side). So the tokio current-thread arm is the one compiled, and the per-thread `MetricSnapshot` premise still holds. Positive control: in the trial, the metric suites that assert `assert_delta(N>0)` passed (e.g. `errors_metric_integration`, `jwks_metrics_integration`, `db_metrics_integration`). I'll name one such passing test in the Layer-4/5 notes at implementation time.

### 10. Test-count parity (@test item 3)
At implementation time I'll record per-crate passed/failed/ignored for ac-service, gc-service, ac-test-utils and gc-test-utils, on 0.8.6 at the start commit and on 0.9.0, in §Devloop Verification Steps. Any drop is a finding.

### 11. TLS / sslmode and connect-error text (@security items 2, 4)
- **Resolved feature set unchanged.** `sqlx`: `_rt-tokio, any, chrono, default, derive, json, macros, migrate, postgres, runtime-tokio, sqlx-macros, sqlx-postgres, uuid`. `sqlx-postgres`: `any, chrono, default, json, migrate, offline, uuid`. Both are byte-identical to 0.8.6's resolved sets. `cargo tree -p sqlx` contains no rustls / native-tls / openssl, so no TLS backend, as before.
- **Default sslmode is still `Prefer`.** `sqlx-postgres-0.9.0/src/options/ssl_mode.rs:18-19` (`#[default] Prefer`), with `PGSSLMODE` read at `options/mod.rs:91`, same as 0.8.6 `:84`. `connection/tls.rs` (0.9.0 lines 23-45) differs from 0.8.6 only by a lifetime elision:
  - `Prefer` with no backend returns the plain socket, as before (`!tls::available()`). There is no new hard error and no new downgrade.
  - `Require`/`VerifyCa`/`VerifyFull` still call `tls::error_if_unavailable()?`. That function (`sqlx-core-0.9.0/src/net/tls/mod.rs`, identical to 0.8.6) returns `Error::Tls("TLS upgrade required by connect options but SQLx was built without TLS support enabled")`, so it is still a hard error and never falls back.
  - Changelog: CHANGELOG.md §0.9.0 (https://github.com/launchbadge/sqlx/blob/main/CHANGELOG.md) lists no sslmode change; the TLS entries are #3861/#4027/#4251 (handshake internals) and #4042 (webpki-roots), none of them reachable without a TLS feature.
- **Connect-error text — CORRECTED: one new leak path in 0.9, see §12.** The URL-parse half below holds. The SCRAM half did not: a second diff of `connection/{sasl,establish,tls}.rs` (prompted by @semantic-guard) found #4201 added `Error::Configuration(format!("Failed to saslprep password: {:?}", error))`. The original analysis follows. The `sqlx::Error` `#[error]` strings in 0.9.0 `error.rs` match 0.8.6 exactly, plus one new `ConfigFile` variant ("error reading configuration file: {0}"), which is reachable only with `sqlx-toml`, and that is off.
  - The `PgConnectOptions::from_str` error constructors in `options/parse.rs` are unchanged from 0.8.6 (diff of every `Error::`/`format!` line is empty). Each wraps a std/url error whose Display carries no input: `url::ParseError`, `Utf8Error` for the percent-decoded user/password/dbname ("invalid utf-8 sequence of N bytes from index M"), `ParseIntError`, `AddrParseError`.
  - The one value-echoing error is `ssl_mode.rs:47-48` `unknown value {s:?} for ssl_mode`, which echoes only the sslmode value and is unchanged.
  - #3800 changed only `PgConnectOptions::options()` (the builder), not the URL parser. #3993's pgpass parse errors now report a line number instead of line content (per @observability), which is an improvement, and we don't use pgpass.
  - Database errors show the server's message ("password authentication failed for user …" names the user, never the password), unchanged apart from the new " at line N" suffix, which is a Postgres C-source line.
  - ~~Conclusion: the logs can stay as they are.~~ Superseded by §12.
- **Passwords:** @security confirmed the dev-cluster password (`infra/services/postgres/secret.yaml`) and the unit-test/compose DB passwords are ASCII, so the #4201 SASLprep fix has no effect.
- **`AssertSqlSafe` sites: none.** Not in production code, test code or test-utils. The two `format!` sites move to `concat!` (§6). The trial build is green with zero wrappers.

### 12. NEW credential-leak path from sqlx 0.9 SASLprep (#4201): redact Configuration errors at connect
**Finding.** `sqlx-postgres-0.9.0/src/connection/sasl.rs:93-101` runs `saslprep(password)`, and on failure returns `Error::Configuration(format!("Failed to saslprep password: {:?}", error))`. In 0.8.6 the password was hashed raw, with no saslprep and no error. The `stringprep 0.1.5` error is `Error(ErrorCause::ProhibitedCharacter(char))` (`lib.rs:19-21,91`), so its Debug **embeds one character of the password**, e.g. `Error(ProhibitedCharacter('\u{7}'))`. The username path (`:57-64`) does the same with a username character, which is lower sensitivity. A password character can reach two sinks:
1. `error!("Failed to connect to database: {}", e)` at `ac-service/src/main.rs:123` and `gc-service/src/main.rs:136`, which goes to Loki;
2. `main() -> Result<(), Box<dyn Error>>` returning the error via `?`, so Rust prints `Error: {Debug}` to stderr, which also ends up in the pod logs.

The trigger is a password containing a stringprep-prohibited character: ASCII and non-ASCII control characters, private-use, non-characters and similar (non-ASCII spaces are mapped to U+0020 first, RFC 4013 §2.1, so they never trigger it). Printable-ASCII passwords, which is every password we have today (per @security), never trigger it. Exposure is one character, but only for passwords that 0.9 can't authenticate with anyway. Low severity, but it is exactly the class the credential-leak check exists for, and this bump introduces it, so it's in scope.

**Final design (settled at Gate 1 by Lead ruling, @dry-reviewer, @code-reviewer, @security, @observability; supersedes the earlier "private fn ×2" proposal).** One feature-gated module, `crates/common/src/db.rs`, owns the whole connect path. Both `add_query_timeout` copies and both inline `PgPoolOptions` blocks are deleted.

- **Feature:** `db = ["dep:sqlx"]` in `crates/common/Cargo.toml`, with `sqlx = { workspace = true, optional = true }`, following the existing `test-utils = ["dep:metrics", …]` pattern. It's enabled only by `ac-service` and `gc-service`, on both the normal and the dev-dep `common` lines. The module is `#[cfg(any(test, feature = "db"))]` and common gets a `sqlx` dev-dependency, so `cargo test -p common` runs its tests (the same reasoning as the existing `metrics` dev-dep). I chose `db` over `postgres` because it names what the module is for, and it's the name used everywhere.
- **Settings:** `pub struct PoolSettings { max_connections, min_connections, acquire_timeout, idle_timeout, max_lifetime, statement_timeout }`. Its documented `Default` (20 / 2 / 5 s / 600 s / 1800 s / 5 s) is the one home for the values, and the connect fn hardcodes nothing. AC and GC each pass `PoolSettings::default()` explicitly at the call site. No new env vars this loop; the struct is the seam for making these config-driven later. AC's inline comments carry over onto the `Default` fields, but **the "ADR-0012" attribution is dropped**: @dry-reviewer checked that `adr-0012-infrastructure-architecture.md` contains none of these numbers. The values are documented as the current operational defaults, identical across AC and GC.
- **Statement timeout:** `PgConnectOptions::from_str(url)` then `.options([("statement_timeout", "<N>ms")])` replaces the URL-string concat (@security item 3, preferred form). `.options()` escaping (#3800) doesn't matter for `statement_timeout=5000ms`, which has no space or backslash. The behavior is proven by test (below). Logging settings are untouched (`log_statements`/`log_slow_statements` keep their defaults, per @observability).
- **Parse errors take the same mapping** (@security a): `PgConnectOptions::from_str(url)` is matched explicitly into the same `sqlx::Error → DbConnectError` function. There is no `?` into a type carrying `sqlx::Error`. Test (c) asserts exactly `DbConnectError::InvalidConfiguration`.
- **Our 5 s statement timeout wins over an operator-supplied one** (@security b): `PgConnectOptions::options()` *appends* `-c statement_timeout=…` after whatever `options=` the URL already set (`sqlx-postgres-0.9.0/src/options/mod.rs:443-451`, push onto the existing string). Postgres applies startup `-c` switches in order, so the last one wins. Test (e) uses the same `#[sqlx::test]` connect options, pre-loaded with `.options([("statement_timeout", "60s")])` to simulate a URL-supplied value, runs them through `connect_pool_with`, and asserts `SHOW statement_timeout` = `5s`. That proves ours wins rather than asserting it. The URL route and the `.options()` route land in the same `PgConnectOptions.options` string (`options/parse.rs:92-98` pushes the raw URL value; `mod.rs:443-451` appends), so the test carries a one-line comment saying so. `#[sqlx::test]` hands over `PgConnectOptions`, not a URL (@security).
- **No `Debug` over secrets** (@semantic-guard): `DbConnectError` derives `Debug` over a unit variant plus `sqlx::Error`, never over the URL or `PgConnectOptions`. `PoolSettings` holds only counts and `Duration`s.
- **API:** `pub async fn connect_pool(database_url: &str, settings: &PoolSettings) -> Result<PgPool, DbConnectError>` parses, then calls `connect_pool_with(PgConnectOptions, &PoolSettings)`, which is also used by the DB test. Eager `connect_with`, never lazy.
- **Error type** (`thiserror`, per ADR-0002/0003):
  - `DbConnectError::InvalidConfiguration`, a unit variant with fixed Display `invalid database configuration (detail redacted: may contain credential material; check the DATABASE_URL secret)`. It holds **no** `sqlx::Error`, no `#[source]` and no `#[from]`: the original is dropped, so neither Display, Debug nor a `source()` chain walk can reach it (@security 2, @semantic-guard 1).
  - `DbConnectError::Connect(#[source]-carrying sqlx::Error)` covers every other variant (Io, Tls, Database, PoolTimedOut…), with Display unchanged, which these keep per §11.
  - There is no `From<sqlx::Error>`. The mapping is one explicit `match` in `db.rs`, so the redaction can't be bypassed.
- **Logging:** `common::db` does **not** log, and isn't `#[instrument]`ed (no URL can become a span field; @security 3). Each `main.rs` keeps `error!("Failed to connect to database: {}", e)`, so the per-service `RUST_LOG` fallback filters (`ac_service=debug,…` / `gc_service=debug,…`, which have no global level) still emit it (@observability 1). It's logged exactly once (@code-reviewer). `main` returns that same already-redacted `DbConnectError` via `?` (@observability 2). The comment at the helper records: the credential-custody exception to error-context preservation; why startup is the only site; and that `connect_lazy` or credential rotation would reopen this.
- **Tests in `common::db`:**
  - (a) A sentinel `sqlx::Error::Configuration("…S3NT1NEL\u{0007}…")`. Positive control: the raw Display and Debug contain `S3NT1NEL`. Then the mapped `DbConnectError` Display AND Debug contain neither `S3NT1NEL` nor `\u{7}`, and `source()` is `None`.
  - (b) An `Error::Io` maps to `Connect`, with the text passing through and `source()` being `Some`.
  - (c) A malformed URL (bad port) through `connect_pool` yields `InvalidConfiguration`. That exercises the real parse path, not just the mapper.
  - (d) `#[sqlx::test(migrations = false)]` takes `PgConnectOptions`, calls `connect_pool_with`, and asserts `SHOW statement_timeout` = `5s` on a pooled connection. This proves the DoS control actually applies, not just that a string contains it (@security item 3).
- **MC/MH must not gain sqlx:** recorded at implementation with per-package `cargo tree -p mc-service -e features -i sqlx` and `-p mh-service`, expected no match. These are per-package, not workspace-level, because a resolver-2 workspace build unifies `common/db` (@code-reviewer 3).
- **Runbooks:** besides the checklist above, the `Failed to connect to database: <error>` symptom lines (`ac-service-deployment.md:625`, `gc-deployment.md:726`) are updated to show the exact redacted string alongside the generic form (@observability 3).

**sqlx's own tracing doesn't emit the error before our redaction** (@security 2; confirmed independently by @observability):
- `PgPoolOptions::connect` → `connect_with` (`sqlx-core-0.9.0/src/pool/options.rs:537-558`) *returns* errors from `try_min_connections(..)?` and `acquire()?` without logging them.
- The per-attempt loop (`pool/inner.rs:337-397`) silently retries only `Io(ConnectionRefused)` and transient `Database` errors. Every other error is `return Err(e)` immediately, with no log.
- sqlx's only connect-error log is `tracing::debug!(%error, "error while maintaining min_connections")` (`inner.rs:436`), in background maintenance. That runs only after a successful startup (the reaper first sleeps `min(idle_timeout, max_lifetime)`, `inner.rs:512-571`). It is DEBUG, and the deployed `RUST_LOG=info,{svc}=debug` and the in-code fallbacks `{svc}=debug,tower_http=debug` keep `sqlx` at INFO or off. No filter change is needed.

**Runbook content** (@operations): `docs/runbooks/ac-service-deployment.md` §Issue 1 (L621) and `docs/runbooks/gc-deployment.md` §Issue 1 (L722) get:
- the exact redacted string, copied from the code, as a grep-able symptom;
- (as landed after Gate-3 review) one sentence that the settings were rejected client-side, either while parsing the URL (no network I/O) or during SCRAM authentication (Postgres was reached), so network policy, DNS and Postgres reachability are not the cause;
- a decoded-DATABASE_URL checklist: numeric port; `sslmode` ∈ disable/allow/prefer/require/verify-ca/verify-full; reserved characters (`@ : / ? # % [ ]`) percent-encoded in user and password; no SASLprep-rejected characters in the user and password (as landed: control characters, private-use or other code points prohibited by RFC 4013; printable ASCII is always accepted). The last item is named as the case the redaction exists for.

**Sweep: every non-test site where a sqlx connect/pool/acquire error is logged, formatted or returned** (Lead item 2):
- *Startup connect:* `ac-service/src/main.rs:114-125`, `gc-service/src/main.rs:127-138`. These are the only `PgPoolOptions`/connect sites outside tests (grep: no `PgPool::connect`, `PgConnection::connect`, `connect_with` or `connect_lazy` in any `crates/*/src`). Both are routed through `common::db`.
- *Runtime acquire-through-query paths*, which can't carry `Configuration`:
  - GC `impl From<sqlx::Error> for GcError` (`errors.rs:284`), GC repository `Result<_, sqlx::Error>` matches (participants, media_handlers, meeting_assignments, meeting_controllers, meetings);
  - AC `AcError::Database(format!(…, e))` sites (repositories service_credentials/signing_keys/organizations/users/auth_events, services key_management/registration, handlers admin `.begin()`, `signing_keys.rs:107 .begin()`);
  - health/readiness `SELECT 1` (gc `handlers/health.rs`, ac `routes/mod.rs`).

  Why none of them can see `Configuration`: the exhaustive list of `Error::Configuration` constructors in sqlx-core/sqlx-postgres 0.9.0 is URL/option parsing (`options/parse.rs`, `ssl_mode.rs:47`), SASLprep (`connection/sasl.rs:61,98`), and rustls cert loading (`net/tls/tls_rustls.rs`, not compiled: no TLS feature). Parsing runs once, inside `connect_pool`. Every later connection the pool opens (acquire, `min_connections` replenishment) reuses the same fixed `PgConnectOptions` (`pool/inner.rs:342-346` clones `self.connect_options`; we never call `set_connect_options`). SASLprep is a pure function of that fixed password, so if it passed on the eager startup connect it passes forever. If it fails, `connect_pool` returns `InvalidConfiguration` and the process exits before serving.
- *Pre-existing, not new in 0.9, not a password path* (from @observability): `sqlx-postgres options/parse.rs` `warn!(%key, %value, "ignoring unrecognized connect parameter")` logs an unknown URL query parameter's value at WARN. Our rendered URLs carry no unknown parameters, and the move to `.options()` removes our own `options=` URL parameter. Recorded for completeness.

**Residual, not fixable in-tree (@security, @operations, @semantic-guard 6; no TODO by agreement):** the sqlx-cli 0.9 SASLprep error echo (`sasl.rs:93-101` formats the stringprep error with `{:?}`) can only fire on a password (or user) containing an RFC 4013-prohibited character (control, private-use and similar; non-ASCII spaces are mapped first and never trigger it). Such a password can't authenticate under 0.9 at all, so it would surface on the first deploy. It is also structurally prevented today: `infra/kind/scripts/provision.sh:439-444` refuses to provision a POSTGRES_USER/PASSWORD/DB outside `[A-Za-z0-9._~-]`, and the db-migrate Job composes its URL from that same secret. Relaxing that charset also widens what a failing db-migrate Job can print. Reporting the `{:?}` upstream is an outward-facing action for the Lead/operator to decide.

**Residual (pre-existing, @security):** sqlx_postgres options/parse.rs logs the value of any unrecognized DATABASE_URL query parameter at WARN (pre-existing since 0.8). Rendered URLs carry credentials only in userinfo, and our only query key is `options=`, so there is no in-tree exposure. No allowlist is added, because it would duplicate sqlx's key set and drift.

### 13. Pre-existing gap filed (requested by @security via Lead): Postgres transport TLS
New `docs/TODO.md` section `## Transport Security — Database`, inserted immediately before `## Multi-Cluster Networking (Production)`, with @security's entry verbatim (AC/GC → Postgres runs plaintext, ADR-0012 §4 verify-full unconfigurable today). This records a pre-existing gap, not a finding against this diff, and the existing `docs/TODO.md` row covers it.

### @security — explicit flags
1. **Credential path, library side**: no `PgConnectOptions`/`PgPoolOptions` API we call changed signature. Behavior changes on paths we *could* hit: #4201 SCRAM `SASLprep` fix (passwords with non-ASCII/mappable chars now normalised per RFC 4013; ASCII unaffected), #3993 `.pgpass` backslash-unescape (we don't use pgpass), whoami v2 `PGUSER` fallback (we always set user). The `statement_timeout` URL option is verified to still apply. Please confirm our generated DB passwords are ASCII, or say if you want a probe.
2. **sqlx-cli supply chain**: 0.9.0 is still installed `--locked` against the crate's shipped lockfile. Upstream stopped tracking it in git, but the published crate still carries one. Same feature set, no TLS backend compiled (as before).
3. **Suppression removal** (§7): your policy content. Removal is what the entry's own "only real exit" clause prescribes.
4. New `hmac` 0.12/0.13 duplicate (§1): resolved by deleting the unused `hmac`/`sha2` deps from ac-service (Lead note). ac-service's HMAC is `ring::hmac`, so no crypto code changes.

### Validation plan
`./scripts/layer-fast.sh` with the 0.9.0 CLI on PATH (see §4 / Pre-Work). Plus: the duplicate check per §1; for the RUSTSEC removal, on the REAL tree before deleting the entry, `grep -c 'name = "rsa"' Cargo.lock` (expect 0) and `cargo tree -p rsa --invert` (expect "did not match"), both recorded; `audit-suppressions-check.sh` OK; Layer-6 `cargo audit` against the real Cargo.lock.

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (incl. §12; conditions: sentinel Display+Debug test, check sqlx pool tracing pre-redaction, startup-only trigger) |
| Test | confirmed (Gate-3 check: per-crate before/after pass counts + named assert_delta(N>0) positive control in main.md) |
| Observability | confirmed (incl. §12) |
| Code Quality | confirmed (incl. common::db, DbConnectError, PoolSettings) |
| DRY | confirmed (conditional: feature-gated common::db owns connect, timeout, pool opts, redaction) |
| Operations | confirmed (incl. §12; adds runbook rows for ac/gc deployment DB-failure sections) |
| Semantic Guard | confirmed (incl. §12; conditions: no source chain, trade-off comment, Display+Debug test, shared helper, sqlx-cli residual recorded) |

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | audit.sh `|| true` masks exit 2; runbook "before network I/O" |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | override test lacks 60s positive control; connect_pool(url) path untested |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | runbook wording; stale #3800 row |
| Code Quality | RESOLVED-FIXED | 4 | 4 | 0 | Connect transparent; connect_pool_with private; single format!; audit.sh mask |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | runbook wording identical; _audit_gate.test.sh setup clone |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | runbook wording + username in checklist |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | runbook wording |

---

## Pre-Work

None. Lead note: specialist INDEX.md and review-protocol.md were injected *by reference* (each teammate's prompt instructs it to Read the file as its first action) rather than pasted inline — same content, avoids ~124 KB of duplicated prompt text across 8 spawns. Semantic Guard spawned: diff plausibly touches connection-option/credential-carrying types.

**Operator decision (2026-10-05):** a temporary in-container install of sqlx-cli matching the Cargo.lock sqlx version is approved for Layers 4/5 until the host rebuild; it must be recorded here (version + install command). The `SQLX_CLI_VERSION_MISMATCH` check is not bypassed. **Remaining operator action at devloop end:** `./infra/devloop/devloop.sh --rebuild --recreate`.

**Temporary sqlx-cli install (implementer, per the Operator decision above).** After the Cargo.lock bump, the version was derived by the one reader:
```
source infra/lib/cargo-lock-version.sh; v="$(cargo_lock_version Cargo.lock sqlx)"   # -> 0.9.0
cargo install sqlx-cli --locked --no-default-features --features postgres --version "=${v}" --root /tmp/claude-1000/-work/1917ea97-6db9-4405-a6bc-70f1d5d55234/scratchpad/sqlx-cli-root
```
Installed `sqlx-cli 0.9.0` (`sqlx --version` → `sqlx-cli 0.9.0`; `awk '{print $2}'` → `0.9.0`, so `test.sh`'s drift check parses it). The Gate-1 trial had already installed this exact version with the same flags into the same root, so cargo reported "already installed". All `layer-fast.sh` runs use `PATH=/tmp/claude-1000/-work/1917ea97-6db9-4405-a6bc-70f1d5d55234/scratchpad/sqlx-cli-root/bin:$PATH`. The in-image `/usr/local/cargo/bin/sqlx` is still **0.8.6**, so without the prefix `test.sh` correctly fails `SQLX_CLI_VERSION_MISMATCH`. The check is not bypassed or edited. The Lead's Gate-2 `layer-all.sh` uses the same prefix. The remaining host action is above.

---

## Implementation Summary

### Dependency
| Item | Before | After |
|------|--------|-------|
| workspace `sqlx` (`Cargo.toml:90`) | `"0.8"` → 0.8.6 | `"0.9"` → 0.9.0 (features unchanged) |
| sqlx crates in Cargo.lock | 7 × 0.8.6 | 7 × 0.9.0, one version each (`grep -A1 'name = "sqlx' Cargo.lock`) |
| `rsa` in Cargo.lock | 0.9.9 (via sqlx-mysql) | **absent** |
| ac-service `hmac = "0.12"`, `sha2 = "0.10"` | declared, unused | removed; only `hmac 0.13.0` remains |
| sqlx-cli (all consumers) | 0.8.6 | 0.9.0, derived from Cargo.lock; no consumer edited |

### Code
- `crates/gc-service/src/handlers/meetings.rs`: `MEETING_SELECT_QUERY` const + `format!` replaced by the file-local `meeting_select_query!()` macro + `concat!`. The SQL is byte-identical and `&'static str`. A comment says why it's a macro.
- `crates/common/src/db.rs` (new, `db` feature): `PoolSettings` with a documented `Default`; public `connect_pool` and a private `connect_pool_with` (Gate 3: no second public entry that skips the redacting parse); `DbConnectError` (`InvalidConfiguration`: unit, no source; `Connect(sqlx::Error)` via `#[error(transparent)]`, so the text isn't duplicated along the source chain). The mapping is one explicit fn, with no `From`. Statement timeout goes through `PgConnectOptions::options()`. 6 tests.
- `crates/{ac,gc}-service/src/main.rs`: the startup connect calls `common::db::connect_pool(&config.database_url, &PoolSettings::default())`. Each main still logs `Failed to connect to database: {}` once and returns the redacted error. Both `add_query_timeout` copies and both inline pool blocks are deleted, and the unsupported "ADR-0012: 5s statement timeout" comment is dropped.

### Additional Changes
- `.sqlx/sqlx-data.json` deleted (dead ≤0.6 artifact, §5); `.dockerignore` comment no longer lists `.sqlx/`.
- RUSTSEC-2023-0071 suppression removed from `audit-suppressions.toml`, and `.cargo/audit.toml` regenerated (`ignore = []`); `.pnpm-audit-ignore.json` regenerated byte-identical. `docs/TODO.md` entries marked resolved with @security's wording; security's `## Transport Security — Database` entry inserted verbatim; `docs/contributor/audit-suppressions.md` and `docs/runbooks/devloop-validation.md` generalised.
- Runbooks `ac-service-deployment.md` / `gc-deployment.md` §Issue 1: the exact redacted symptom string (grep-verified identical to `db.rs`) plus a DATABASE_URL checklist.
- **Found at Layer 6, fixed in-loop:** `scripts/lang/rust/audit.sh` aborted (`wrapper-aborted-early-exit-1`) when `.cargo/audit.toml` has `ignore = []`. Its `SUPPRESSED=` `grep | sort | paste` exits 1 on no match under `pipefail`. Removing the last suppression is the first time the tree hit this state. Fix: `{ grep … || true; }`. Regression test: `scripts/lang/_audit_gate.test.sh` Part 4 (empty ignore, plus a one-ignore positive control). Controls (@team-lead ask: prove the scanner actually ran, not just STATUS=OK). The PATH-stubbed `cargo` records its argv, and "scanner invoked" holds only if it was called with exactly `audit`:
  - Gate 3 (@security F1, @code-reviewer 4): `|| true` narrowed to `|| [[ $? -eq 1 ]]`, so only grep's no-match is tolerated and a read error (exit 2) still aborts. New case: a PATH-stubbed `grep` that exits 2 for `.cargo/audit.toml` only, which works as any uid. It must give a nonzero exit, no scanner call and `STATUS=FAIL`. Part 3/4 setup was extracted into one `make_wrapper_repo` + `run_wrapper` (@dry-reviewer F2).
  - with the fix: 54 passed, 0 failed;
  - `|| true` variant (the Gate-2 fix): 51 passed, 3 failed (all three unreadable-case assertions), so masking is now caught;
  - negative control, HEAD's `audit.sh`: 51 passed, 3 failed (empty-ignore STATUS, exit 0, scanner invoked);
  - mutation control, `cargo audit` replaced by a bare `emit_status OK cargo-audit-passed` (faked green): 52 passed, 2 failed (both "scanner invoked"). So a misplaced `|| true` or skip can't pass as green.

---

## Files Modified

```
 .cargo/audit.toml                          |   4 +-
 .dockerignore                              |   4 +-
 .sqlx/sqlx-data.json                       |   3 -
 Cargo.lock                                 | 541 ++++++++++-------------------
 Cargo.toml                                 |   2 +-
 audit-suppressions.toml                    |  57 ---
 crates/ac-service/Cargo.toml               |   8 +-
 crates/ac-service/src/main.rs              |  33 +-
 crates/common/Cargo.toml                   |  10 +
 crates/common/src/lib.rs                   |   6 +
 crates/gc-service/Cargo.toml               |   5 +-
 crates/gc-service/src/handlers/meetings.rs |  20 +-
 crates/gc-service/src/main.rs              |  32 +-
 docs/TODO.md                               |  24 +-
 docs/contributor/audit-suppressions.md     |   5 +-
 docs/runbooks/ac-service-deployment.md     |   7 +
 docs/runbooks/devloop-validation.md        |   2 +-
 docs/runbooks/gc-deployment.md             |   7 +
 scripts/lang/_audit_gate.test.sh           | 111 +++++-
 scripts/lang/rust/audit.sh                 |   5 +-
 20 files changed, 375 insertions(+), 511 deletions(-)
 crates/common/src/db.rs (new, untracked) | 250 lines
```

---

## Devloop Verification Steps

### Gate 2 — full `layer-all.sh` (Lead, final tree)
Run with `PATH=<scratchpad>/sqlx-cli-root/bin:$PATH` (sqlx-cli 0.9.0, operator-approved; host image still 0.8.6). `DEVLOOP_FMT_APPLY=1`, no FMT_APPLIED.

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 5s |
| 2 Format | OK | 4s |
| 3 Guards | OK | 131s |
| 4 Test | N/A aggregate (cargo-test-passed, nx-test-passed; proto intentional-gap N/A) | 381s |
| 5 Lint | OK | 1s |
| 6 Audit | N/A aggregate (cargo-audit-passed, pnpm-audit-passed, buf-breaking-passed; proto N/A) | 3s |
| 7 Env-tests | OK (env-tests-passed, browser-e2e-passed) | 1202s |

`TOTAL_RESULT=N/A`, exit 0. Live rollforward: in-cluster `db-migrate` Job (image built with `SQLX_CLI_VERSION` from Cargo.lock = 0.9.0) Completed over the existing 0.8-migrated DB; `_sqlx_migrations` = 10 rows, max 20261004000002, all success.

`./scripts/layer-fast.sh` was run with `PATH=/tmp/claude-1000/-work/1917ea97-6db9-4405-a6bc-70f1d5d55234/scratchpad/sqlx-cli-root/bin:$PATH` (sqlx-cli 0.9.0; see Pre-Work). Run 3 is **green**: `TOTAL_RESULT=N/A`, exit 0; L1 OK, L2 OK, L3 OK, L4 N/A (aggregate; `cargo-test-passed`, `nx-test-passed`), L5 OK (`cargo-clippy-passed`), L6 N/A (aggregate; `cargo-audit-passed`, `pnpm-audit`). Two earlier runs failed:
- Run 1, L5: clippy `doc_markdown` on two un-backticked "SASLprep" in `db.rs` doc comments. Fixed.
- Run 2, L6: `wrapper-aborted-early-exit-1` from `audit.sh` on the empty ignore list. Fixed; see Additional Changes.

Layer 4/5 migrated the test DB through `test.sh` with the 0.9.0 CLI ("Applying database migrations (pending-only)…", a no-op over the 0.8-migrated DB).

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Simple Guards
**Status**: ALL PASS (incl. cross-boundary scope, audit-suppressions check `suppressions-clean`, `audit-gate-test-passed`)

### Layer 4: Unit Tests
**Status**: PASS (`cargo-test-passed`)

**Per-crate parity (@test)**, `cargo test -p <crate> --no-fail-fast` against the devloop DB:

| Crate | 0.8.6 (start commit) passed/failed/ignored | 0.9.0 passed/failed/ignored |
|---|---|---|
| ac-service | 1004 / 0 / 0 | 1004 / 0 / 0 |
| gc-service | 1056 / 0 / 0 | 1056 / 0 / 0 |
| ac-test-utils | 17 / 0 / 15 | 17 / 0 / 15 |
| gc-test-utils | 7 / 0 / 2 | 7 / 0 / 2 |
| common | — | 254 / 0 / 9 (includes 6 new `db::tests`) |

No drop. **Positive control (current_thread / non-vacuous metric assertions):** `crates/ac-service/tests/errors_metric_integration.rs::handle_service_token_invalid_grant_type_emits_authentication_category` (a `#[sqlx::test]` asserting `assert_delta(1)` on its category) passes under 0.9.

`common::db` tests:
- `configuration_error_is_redacted_in_display_debug_and_source`
- `non_configuration_error_passes_through_with_source`
- `malformed_url_is_redacted_through_the_real_parse_path` (asserts exactly `InvalidConfiguration`)
- `connect_pool_applies_statement_timeout_via_the_url_path`: the production entry, plain `DATABASE_URL` → `5s`
- `connect_pool_overrides_a_statement_timeout_in_the_url`: `DATABASE_URL` + `options=-c statement_timeout=45s`. Control: a plain pool on that URL → `45s`; `connect_pool` → `5s`
- `connect_pool_with_overrides_a_statement_timeout_set_via_options`: `.options()` 45s. Control → `45s`; `connect_pool_with` → `5s`

(Gate 3, @test F1/F2: the 45s controls prove the override isn't vacuous, since the Postgres default is 0, and the URL path is now exercised directly. I used 45s rather than 60s because `SHOW` normalises 60s to `1min`.)

All pass.

### Layer 5: All Tests (Integration) / Clippy
**Status**: PASS (`cargo-clippy-passed`, `-D warnings`, `--all-targets`)

### Layer 6: Audit
**Status**: PASS (`cargo-audit-passed` on the real tree's Cargo.lock, no suppressions configured; the remaining output is the pre-existing allowed warnings only)

### Evidence recorded (Lead / reviewer asks)
- **rsa absent, real tree (@security):** `grep -c 'name = "rsa"' Cargo.lock` → `0`. `cargo tree -p rsa --invert` → ``error: package ID specification `rsa` did not match any packages``, and the same with `--target all`. Captured in /work after `cargo update -p sqlx`, before the suppression was deleted.
- **sqlx duplicates:** `grep -A1 'name = "sqlx' Cargo.lock` → 7 packages, all `0.9.0`. (`cargo tree -d | grep sqlx` lists sqlx only as a *dependent* of other duplicated crates; see §1.)
- **MC/MH don't pull sqlx (per-package, @code-reviewer 3 / @security 5):** `cargo tree -p mc-service -e features -i sqlx` → ``error: package ID specification `sqlx` did not match any packages``, and the same for `-p mh-service`. `cargo tree -p {mc,mh}-service --target all | grep -c sqlx` → `0`. Positive control: `cargo tree -p ac-service -e features -i sqlx` shows `common feature "db"`.
- **CLI version parse (@test 4 / @operations 4a):** `sqlx --version` → `sqlx-cli 0.9.0`, so `awk '{print $2}'` → `0.9.0`.
- **0.9 CLI over a 0.8-migrated DB (@operations 4b):** §3 (no-op, identical rows/checksums), re-confirmed by Layer 4's pending-only migrate on the shared test DB.
- **Runbook strings:** `grep -cF` of the redacted message → 1 hit each in `db.rs`, `ac-service-deployment.md`, `gc-deployment.md`.

### Layer 7: Env-tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Wall-clock time for dev-cluster rebuild + env-test run; pass/fail summary; log path}

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Test Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

Verified clean:
- sqlx 0.9 statement logging is unchanged (QueryLogger defaults: DEBUG, slow WARN at >1s, SQL text only, no bind values). `connect_with` keeps those defaults.
- No `AssertSqlSafe`. The meetings.rs statements are static `concat!`, so no value can reach the slow-statement WARN, which in-cluster `RUST_LOG=info` emits.
- `common::db` doesn't log. Each `main.rs` logs the connect failure once under its own target, so the `RUST_LOG`-unset fallback filters still emit it.
- The returned `InvalidConfiguration` is redacted in Display, Debug and `source()`, tested with a sentinel and a positive control.
- `record_db_query` status labels stay bounded literals. Pool values are unchanged.
- The `aquired_after_secs` → `acquired_after_secs` field rename has no consumers.

Findings (both fixed):
1. The runbook premise "rejected before any network I/O" was wrong for the SASLprep case (`ac-service-deployment.md:626`, `gc-deployment.md:727`). Reworded to "during parsing or authentication".
2. The §2 #3800 row was stale after §12 (it cited the deleted `add_query_timeout`). It now records the change as hit and cites the `statement_timeout` tests.

### Code Quality Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### DRY Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED

**True duplication findings** (entered fix-or-defer flow):
{List findings sent to implementer, or "None"}

**Extraction opportunities** (appended to `docs/TODO.md`):
{One bullet per `docs/TODO.md` entry added, citing the section heading the entry was added under, or "None"}

### Operations Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

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

**This devloop**:
- (none surfaced in this devloop) — all 14 Gate-3 findings fixed. Recorded residuals (not deferrals, see §12): sqlx-cli SASLprep echo in db-migrate (blocked by `provision.sh:439-444` charset check); pre-existing sqlx parse.rs WARN on unknown URL params. Pre-existing Postgres-TLS gap filed as new `docs/TODO.md` §Transport Security — Database.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `5d04dfb59b39915062828b3e02278ba0b2ef600d`
2. Review all changes: `git diff 5d04dfb59b39915062828b3e02278ba0b2ef600d..HEAD`
3. Soft reset (preserves changes): `git reset --soft 5d04dfb59b39915062828b3e02278ba0b2ef600d`
4. Hard reset (clean revert): `git reset --hard 5d04dfb59b39915062828b3e02278ba0b2ef600d`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit. Unsafe partials: reverting `crates/common/src/db.rs`/the main.rs call sites while keeping sqlx 0.9 reopens the SASLprep password-char leak; reverting `scripts/lang/rust/audit.sh` while keeping the emptied `.cargo/audit.toml` makes Layer 6 abort. Rollback = `git revert` + devloop/db-migrate image rebuild; no DB action either direction (`_sqlx_migrations` identical across 0.8.6/0.9.0). (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

### Issue 2: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

{Add more issues as needed, or "None" if no issues}

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
