# Security Audit - Cross-Review Responses

**Date**: 2025-12-17
**Status**: Complete

All specialists respond to Security's self-audit, accepting/rejecting handoffs and identifying additional responsibilities.

---

## Test Specialist Response

### Handoff Decisions

**[ACCEPT]** Test coverage metrics (95% target)
- I own measurement and reporting via `cargo llvm-cov`
- I track in CI and fail builds that regress below thresholds
- **Security must tag** which code is "security-critical" so I know where to enforce 95% vs 90%

**[ACCEPT]** Actual test implementation
- Security defines **what** to test (threat scenarios, acceptance criteria)
- I implement the actual test code
- Example: Security says "test JWT algorithm confusion". I write the test.

**[SHARED]** Fuzz harnesses (rejecting simple handoff)
- **Security**: Defines which parsers are security-critical, what inputs to fuzz, corpus design
- **Me (Test)**: Writes the actual fuzz harness code (cargo-fuzz boilerplate)

### Answers to Security's Questions

1. **Who writes security tests?**
   - Security defines threat model and acceptance criteria
   - I (Test) write the actual test code

2. **Who measures 95% coverage?**
   - I measure and report
   - Security must tag which code is "security-critical"

3. **Who owns fuzz harnesses?**
   - Shared: Security defines targets; I implement harnesses

### Additional Responsibilities for Security

1. **Security test case design for edge cases** - Security should enumerate cases like "JWT with exp = i64::MAX"
2. **Approval of test data containing sensitive patterns** - Security approves realistic-looking test credentials
3. **Security test prioritization (P0/P1/P2)** - Security assigns priority levels
4. **Review of test mocks for security-critical dependencies** - If I mock bcrypt for faster tests, Security verifies it's safe

---

## Code Reviewer Response

### Handoff Decisions

**[ACCEPT]** Code quality/Rust idioms
- I review for Rust best practices, API design, maintainability
- **I flag security concerns for Security specialist review**

### Answers to Security's Questions

1. **Should I verify security-related lints are enabled?**
   - **YES** - I verify `#![deny(unsafe_code)]`, security-relevant Clippy lints
   - **Security should define the list** of required lints in an ADR

2. **Should I flag new crypto library additions?**
   - **YES** - I flag for Security review:
     - New crypto library dependencies
     - New `unsafe` blocks
     - Deserialization of untrusted data
     - Network protocol parsers
     - Any use of randomness

### Additional Responsibilities for Security

1. **Maintain "watchlist" of security-sensitive patterns** - Provide regex list for me to flag
2. **Define approved crypto libraries allowlist** - I auto-reject non-approved libraries
3. **Security review of error messages** - I check code quality; Security verifies no info leakage

### Gap Identified

**Who reviews algorithmic complexity for DoS potential?**
- Proposed: I flag O(n²) on user input; Security assesses exploitability

---

## Database Specialist Response

### Handoff Decisions

**[ACCEPT]** Database query implementation
- I implement queries and schema
- Security defines requirements; I ensure implementation compliance

### Answers to Security's Questions

1. **Do I verify org_id in every query?**
   - **YES** - Every SELECT/UPDATE/DELETE includes `WHERE org_id = $1`
   - sqlx compile-time checking prevents injection
   - Could add CI grep check (fragile but possible)

### Additional Responsibilities for Security

1. **Define multi-tenancy isolation requirements** - ADR specifying which tables need org_id, exceptions
2. **Audit row-level security (RLS) policies** if we adopt them
3. **Define encryption-at-rest requirements** - Which columns need encryption
4. **Review database credential rotation strategy**
5. **Define SQL injection test scenarios** - Even though sqlx prevents it, we should test

### Gap Identified

**Who owns database connection security (TLS, certs)?**
- Proposed: Security defines requirements; I implement client configuration

---

## Observability Specialist Response

### Handoff Decisions

**[ACCEPT]** Observability implementation
- I implement instrumentation (tracing, metrics, logging)
- Security reviews for PII and defines what's safe to log

### Answers to Security's Questions

1. **Should I have CI grep for UNSAFE fields in `#[instrument]`?**
   - **YES** - I'll add:
     ```bash
     grep -r "#\[instrument\]" | grep -v "skip(password, token, secret, private_key)"
     ```
   - **Security provides the list** of sensitive field patterns

### Additional Responsibilities for Security

1. **Maintain PII/sensitive field blocklist** - Regex patterns for fields to never log
2. **Define log sanitization requirements** - How to redact sensitive values
3. **Review metric cardinality for DoS** - Can attacker cause unbounded cardinality?
4. **Define audit log requirements** - What security events MUST be logged
5. **Review distributed tracing for info leakage** - IDOR risks from exposed trace IDs

---

## Operations Specialist Response

### Handoff Decisions

**[ACCEPT]** Deployment config (HTTPS)
- Security defines requirements (HTTPS-only, TLS 1.3+, mTLS)
- I ensure runtime compliance via deployment configs

### Answers to Security's Questions

1. **Who verifies deployment configs comply?**
   - **Me** - I verify:
     - IaC includes HTTPS/TLS configs
     - Load balancers reject HTTP
     - mTLS configured for service-to-service
     - Secrets injected securely
   - **Security defines the baseline** (TLS version, cipher suites)

### Additional Responsibilities for Security

1. **Define security baseline for deployments** - TLS versions, cipher allowlist, cert requirements
2. **Review IaC for security misconfigs** - Overly permissive IAM, public exposure
3. **Define secret management requirements** - Which secrets manager, rotation policy
4. **Incident response runbooks** - What to do during security incidents
5. **Review container image security** - Base images, vulnerability scanning

### Gap Identified

**Who owns runtime security monitoring (intrusion detection)?**
- Proposed: Security defines threat signatures; Observability monitors; Operations responds

---

## Infrastructure Specialist Response

### Handoff Decisions

**[ACCEPT]** Deployment config (HTTPS) - shared with Operations
- I provision infrastructure (VPCs, firewalls, load balancers)
- Security defines requirements; I implement in infrastructure

### Additional Responsibilities for Security

1. **Define network segmentation requirements** - Trust boundaries, which services communicate
2. **Review cloud IAM policies** - Least-privilege for service accounts
3. **Define DDoS protection requirements** - Rate limits at infrastructure layer
4. **Certificate management policy** - Automated rotation, cert pinning
5. **Data residency and compliance** - GDPR regional restrictions

### Gap Identified

**Who owns cloud security posture management (CSPM)?**
- Proposed: Security defines baseline; Infrastructure scans; Operations remediates

---

## Auth Controller Specialist Response

### Handoff Decisions

**[ACCEPT]** Rate limiting implementation
- I implement the token bucket algorithm and middleware
- Security defines thresholds, backoff strategies, which endpoints

### Additional Responsibilities for Security

1. **Define rate limit thresholds per endpoint** - requests/minute per client_id, IP
2. **Review auth flow for timing attacks** - I implement constant-time; Security audits
3. **Approve cryptographic algorithm choices** - I implement EdDSA; Security approves
4. **Review session management** (future) - Session fixation, CSRF

---

## Cross-Cutting Gaps Identified

### No One Currently Owns

1. **Dependency CVE scanning**
   - Checking `Cargo.lock` for vulnerabilities
   - **Proposed**: Security defines policy; Operations runs `cargo audit` in CI

2. **Secrets scanning (preventing credential commits)**
   - **Proposed**: Security defines patterns; Operations implements pre-commit hooks (gitleaks)

3. **Runtime security monitoring**
   - **Proposed**: Security defines; Observability monitors; Operations responds

4. **Cloud security posture management**
   - **Proposed**: Security baseline; Infrastructure scans; Operations remediates

---

## Consensus Summary

| Item | Final Owner | Security's Role |
|------|-------------|-----------------|
| Test coverage measurement | Test | Define 95% threshold, tag security-critical code |
| Security test implementation | Test | Define threat scenarios and acceptance criteria |
| Fuzz harness code | Test | Define targets, corpus, fuzzing strategy |
| Code quality/Rust idioms | Code Reviewer | N/A (CR flags patterns for Security) |
| Crypto library additions | Code Reviewer → Security | Maintain allowlist, approve additions |
| HTTPS deployment config | Operations/Infrastructure | Define TLS baseline |
| Database query implementation | Database | Define org_id policy, audit RLS |
| Multi-tenancy isolation | Database | Define requirements, audit compliance |
| Observability instrumentation | Observability | Define PII blocklist, audit log requirements |
| Rate limiting implementation | Auth Controller | Define thresholds, backoff strategies |
| Dependency CVE scanning | Operations | Define vulnerability acceptance policy |
| Secrets scanning | Operations | Define secret patterns to detect |

---

## Action Items for Security

Based on cross-review, Security must create:

1. **Security-critical code tags** - For Test to know where 95% coverage applies
2. **Sensitive pattern blocklist** - Regex for PII/credentials for Observability grep
3. **Crypto library allowlist** - Approved libraries for Code Reviewer to enforce
4. **Security lint requirements** - Which Clippy lints must be enabled
5. **TLS baseline specification** - Versions, ciphers for Operations
6. **Vulnerability acceptance policy** - CVE severity thresholds for Operations
7. **Flagging protocol** - How Code Reviewer escalates to Security (PR comment format?)
