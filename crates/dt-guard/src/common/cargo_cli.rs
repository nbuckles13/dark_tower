//! Cargo command-line reading, POLICY-FREE: joining `\`-continued lines into
//! logical lines, and extracting the profile and packages a `cargo` invocation
//! selects. Shared by `release_build_profile` (Dockerfiles, CI) and
//! `tool_bin_paths` (every instruction file), which before this module each
//! carried their own joiner and profile-flag tokenizer.

/// Join `\`-continued physical lines into logical lines, keeping the line
/// number of the FIRST physical line. Each piece is trimmed, so a continuation
/// reads as one space-separated command.
///
/// Without this a `cargo build \` / `    --release ...` split — legal in
/// Dockerfiles and shell — reads as a `cargo build` with no profile flag, and
/// a guard keyed on that false-fails (or misses). Trailing whitespace after the
/// `\` is tolerated (Dockerfile semantics); bash would not treat `\<space>` as a
/// continuation, which only makes this join slightly MORE inclusive.
#[must_use]
pub fn logical_lines(content: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    for (idx, raw) in content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed_end = raw.trim_end();
        let continues = trimmed_end.ends_with('\\');
        let body = trimmed_end.strip_suffix('\\').unwrap_or(trimmed_end);
        match pending.as_mut() {
            Some((_, acc)) => {
                acc.push(' ');
                acc.push_str(body.trim());
            }
            None => pending = Some((line_no, body.trim().to_string())),
        }
        if !continues {
            if let Some(entry) = pending.take() {
                out.push(entry);
            }
        }
    }
    if let Some(entry) = pending.take() {
        out.push(entry);
    }
    out
}

/// The profile a cargo invocation selects, if it names one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedProfile {
    /// `--release`, `-r`, or `--profile release`.
    Release,
    /// `--profile <name>` for some non-release name (quotes trimmed, so a
    /// shell-variable profile reads as e.g. `$DT_TOOLS_PROFILE`).
    Named(String),
    /// No profile flag at all — cargo defaults to `dev`.
    None,
}

/// Extract the profile a cargo command line selects (the first selection).
///
/// Accepts BOTH `--release` and `--profile release`: they are semantically
/// identical, and requiring the literal `--release` would false-fail a
/// legitimate release build expressed the long way.
#[must_use]
pub fn selected_profile(cmd: &str) -> SelectedProfile {
    let tokens: Vec<&str> = cmd.split_whitespace().map(trim_token).collect();
    // `enumerate` rather than an index loop: the lookahead for
    // `--profile <name>` needs the position, but nothing here should be able
    // to index out of range (ADR-0002 no-panic).
    for (i, t) in tokens.iter().enumerate() {
        if *t == "--release" || *t == "-r" {
            return SelectedProfile::Release;
        }
        if let Some(name) = t.strip_prefix("--profile=") {
            return profile_from_name(name);
        }
        if *t == "--profile" {
            if let Some(name) = tokens.get(i + 1) {
                return profile_from_name(name);
            }
        }
    }
    SelectedProfile::None
}

fn profile_from_name(name: &str) -> SelectedProfile {
    let name = name.trim_matches(['"', '\'']);
    if name == "release" {
        SelectedProfile::Release
    } else {
        SelectedProfile::Named(name.to_string())
    }
}

/// The packages a cargo command line names: `-p X`, `-pX`, `--package X`,
/// `--package=X` (quotes trimmed).
#[must_use]
pub fn selected_packages(cmd: &str) -> Vec<String> {
    let tokens: Vec<&str> = cmd.split_whitespace().map(trim_token).collect();
    let mut out = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let name = if *t == "-p" || *t == "--package" {
            tokens.get(i + 1).copied()
        } else if let Some(v) = t.strip_prefix("--package=") {
            Some(v)
        } else {
            t.strip_prefix("-p")
                .filter(|v| !v.is_empty() && !v.starts_with('-'))
        };
        if let Some(n) = name {
            out.push(n.trim_matches(['"', '\'']).to_string());
        }
    }
    out
}

/// Strip wrapping punctuation a command picks up when quoted in prose or YAML
/// (backticks, parentheses, trailing `,`/`;`/`.`/`:`). Shell quotes are left to
/// the name extractors, which trim them from the VALUE only.
fn trim_token(tok: &str) -> &str {
    tok.trim_matches(|c: char| matches!(c, '`' | '(' | ')' | ',' | ';' | ':' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_lines_joins_backslash_continuations() {
        let src = "RUN cargo build \\\n    --release \\\n    --package mh-service\nFROM x\n";
        let lines = logical_lines(src);
        assert_eq!(lines[0].0, 1, "line number is the FIRST physical line");
        assert_eq!(lines[0].1, "RUN cargo build --release --package mh-service");
        assert_eq!(lines[1], (4, "FROM x".to_string()));
    }

    #[test]
    fn selected_profile_all_forms() {
        for (cmd, want) in [
            (
                "cargo build --release --package mh-service",
                SelectedProfile::Release,
            ),
            ("cargo build -r -p x", SelectedProfile::Release),
            (
                "cargo build --profile release -p mh-service",
                SelectedProfile::Release,
            ),
            ("cargo build --profile=release", SelectedProfile::Release),
            (
                "cargo build --profile quick",
                SelectedProfile::Named("quick".into()),
            ),
            (
                "cargo build --profile \"$DT_TOOLS_PROFILE\" -p \"$crate\"",
                SelectedProfile::Named("$DT_TOOLS_PROFILE".into()),
            ),
            ("`cargo build --release -p x`.", SelectedProfile::Release),
            ("cargo build --package mh-service", SelectedProfile::None),
        ] {
            assert_eq!(selected_profile(cmd), want, "{cmd}");
        }
    }

    #[test]
    fn selected_packages_all_forms() {
        assert_eq!(
            selected_packages("cargo build -p a -pb --package c --package=d -r"),
            vec!["a", "b", "c", "d"]
        );
        assert_eq!(
            selected_packages("`cargo build -p dt-story`."),
            vec!["dt-story"]
        );
        assert_eq!(
            selected_packages("cargo build -p \"$crate\""),
            vec!["$crate"]
        );
        assert!(selected_packages("cargo build --profile x").is_empty());
    }
}
