#!/usr/bin/env python3
"""Extract the `config.yaml: |` block from the collector ConfigMap.

Exists so the acceptance harness runs the COMMITTED collector config rather than
a copy of it. A harness with its own config proves something about the copy.

Fails loudly (non-zero, message on stderr) rather than emitting an empty or
partial config: a truncated config would start a collector with a smaller
pipeline, and the harness would then report on controls that were not present.
"""
import sys


def extract(text: str) -> str:
    lines = text.split("\n")
    out: list[str] = []
    grabbing = False
    for line in lines:
        if not grabbing:
            if line.strip().startswith("config.yaml: |"):
                grabbing = True
            continue
        # The block ends at the first non-blank line that is not indented into it.
        if line.strip() and not line.startswith("    "):
            break
        out.append(line[4:] if line.startswith("    ") else line)
    return "\n".join(out).strip("\n") + "\n"


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: extract_config.py <configmap.yaml>", file=sys.stderr)
        return 2
    with open(sys.argv[1], encoding="utf-8") as fh:
        body = extract(fh.read())

    # Structural sanity, not style: these four keys must all be present or the
    # extraction silently lost part of the pipeline.
    required = ("receivers:", "processors:", "exporters:", "pipelines:")
    missing = [k for k in required if k not in body]
    if missing:
        print(
            f"extract_config: extracted block is missing {missing} — the "
            f"`config.yaml: |` block in {sys.argv[1]} did not parse as expected. "
            "Refusing to emit a partial config.",
            file=sys.stderr,
        )
        return 2

    sys.stdout.write(body)
    return 0


if __name__ == "__main__":
    sys.exit(main())
