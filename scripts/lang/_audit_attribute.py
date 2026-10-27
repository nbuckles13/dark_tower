#!/usr/bin/env python3
"""Attribute audit findings to the change under test (ADR-0033 §11, 2026-10-26 amendment).

Usage: _audit_attribute.py <cargo|pnpm> <base-lock-path-or-empty> <assume-preexisting:0|1>
stdin: one finding per line, TAB-separated: <advisory id> <package> <version> [<title> [<source>]]

A finding is PREEXISTING when the same package at the same version is already in the
lockfile at the diff base: the change did not bring it in, and the scheduled full scan
(audit-scheduled.yml) owns it. For cargo the package's SOURCE must match too: a crate
patched to a same-version git fork on the base is not the crates.io crate a change that
drops the patch brings back. Anything else is NEW and blocks. Fail-closed throughout:
no base lock (empty path) means every finding is NEW, and a base lock that parses to
nothing matches nothing. assume-preexisting=1 is for a graph with no base to compare
(a freshly resolved, untracked lock whose manifest the change did not touch).

Prints `AUDIT_FINDING=<NEW|PREEXISTING> <id> <package>@<version> <title>` per finding.
Exit: 0 no NEW finding, 1 at least one NEW, 2 bad input.
"""
import re
import sys


def cargo_packages(text):
    # (name, version, source) per [[package]] table only — not [[patch.unused]], which
    # lists crates that are NOT in the graph. A path dependency has no source line ("").
    out = set()
    for table in re.split(r"^\[\[", text, flags=re.M)[1:]:
        if not table.startswith("package]]"):
            continue
        fields = dict(re.findall(r'^(name|version|source) = "([^"]*)"', table, re.M))
        if "name" in fields and "version" in fields:
            out.add((fields["name"], fields["version"], fields.get("source", "")))
    return out


def pnpm_packages(text):
    # pnpm-lock v9 keys in the top-level packages:/snapshots: sections only, e.g.
    # `  vitest@5.0.5(…):` or `  '@scope/pkg@1.2.3':`. Other sections are skipped:
    # overrides:/patchedDependencies: keys name versions the graph does NOT contain.
    # The lock can hold two YAML documents, pnpm's own toolchain lock first; only the
    # last one (the project's) is the audited graph.
    text = re.split(r"^---$", text, flags=re.M)[-1]
    out, section = set(), None
    key = re.compile(r"^  '?((?:@[^/@\s']+/)?[^@\s'(:]+)@([^\s'(:]+)")
    for line in text.splitlines():
        if line and not line[0].isspace():
            section = line.split(":", 1)[0]
            continue
        if section in ("packages", "snapshots"):
            m = key.match(line)
            if m:
                out.add((m.group(1), m.group(2)))
    return out


PARSERS = {"cargo": cargo_packages, "pnpm": pnpm_packages}


def main(argv):
    if len(argv) != 4 or argv[1] not in PARSERS or argv[3] not in ("0", "1"):
        print("usage: _audit_attribute.py <cargo|pnpm> <base-lock-or-empty> <0|1>", file=sys.stderr)
        return 2
    eco, base_path, assume = argv[1], argv[2], argv[3] == "1"
    base = set()
    if base_path:
        try:
            with open(base_path, encoding="utf-8") as f:
                base = PARSERS[eco](f.read())
        except OSError as e:
            print(f"AUDIT_ATTRIBUTION: cannot read base lock {base_path}: {e} — every finding counts as NEW")
    new = 0
    for line in sys.stdin:
        line = line.rstrip("\n")
        if not line:
            continue
        fields = line.split("\t")
        if len(fields) < 3 or not all(fields[:3]):
            print(f"AUDIT_ATTRIBUTION: malformed finding row: {line!r}")
            return 2
        adv, name, version = fields[:3]
        title = fields[3] if len(fields) > 3 else ""
        ident = (name, version, fields[4] if len(fields) > 4 else "") if eco == "cargo" else (name, version)
        pre = assume or ident in base
        new += not pre
        print(f"AUDIT_FINDING={'PREEXISTING' if pre else 'NEW'} {adv} {name}@{version} {title}".rstrip())
    return 1 if new else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
