# Agent Teams Configuration

This directory contains spawn templates for Agent Teams integration with Dark Tower's specialist system.

## Directory Structure

```
agent-teams/
├── README.md              # This file
├── protocols/             # Context-specific behavior
│   ├── review.md          # How to behave as a dev-loop reviewer
│   └── debate.md          # How to behave in a design debate
└── specialists/           # Specialist identities (who they are)
    ├── security.md
    ├── test.md
    ├── code-reviewer.md
    ├── dry-reviewer.md
    ├── operations.md
    ├── auth-controller.md
    ├── global-controller.md
    ├── meeting-controller.md
    ├── media-handler.md
    ├── protocol.md
    ├── database.md
    ├── infrastructure.md
    └── observability.md
```

## Design Principles

### Separation of Concerns

**Specialist files** define WHO the specialist is:
- Domain ownership
- Core principles (timeless)
- What they review/own
- Pointers to knowledge files

**Protocol files** define HOW they behave in context:
- Communication patterns
- Output formats
- Verdict/scoring formats

This prevents drift between debate and review behavior for the same specialist.

### Timeless vs Current

Specialist files contain **timeless principles**:
- "Use established crypto libraries, never roll your own"
- "Validate at every boundary"

Current specifics live in **knowledge files**:
- "Approved algorithms: Ed25519, AES-256-GCM"
- Injected via `{{inject: path/to/file.md}}`

This keeps templates stable while knowledge evolves.

## Usage

### Spawning a Reviewer

Compose: specialist + review protocol + knowledge

```
You are reviewing code for Dark Tower.

[contents of specialists/security.md]

[contents of protocols/review.md]
```

### Spawning a Debate Participant

Compose: specialist + debate protocol + knowledge

```
You are participating in a Dark Tower design debate.

[contents of specialists/security.md]

[contents of protocols/debate.md]
```

### Spawning an Implementer

Compose: specialist + task context + knowledge

```
You are implementing a feature for Dark Tower.

[contents of specialists/meeting-controller.md]

Task: [description from Lead]
```

## Knowledge Injection

Templates reference knowledge files with:
```
{{inject: docs/specialist-knowledge/security/patterns.md}}
```

The spawning skill replaces these with file contents at spawn time.

If a knowledge file doesn't exist, the injection is skipped (specialist works without that knowledge until it's captured).

## Updating Templates

**When to update specialist files**:
- New domain responsibilities
- Changed ownership boundaries
- New principles (rare)

**When to update protocol files**:
- Changed communication format
- New verdict/scoring rules
- Process improvements

**When to update knowledge files**:
- New patterns discovered
- Gotchas encountered
- Integration lessons learned
- Approved lists updated (crypto, etc.)

## Related Files

- `.claude/agents/*.md` - Original specialist definitions (more verbose)
- `docs/specialist-knowledge/` - Dynamic knowledge files
- `.claude/skills/dev-loop/` - Dev-loop skill using these templates
- `.claude/skills/debate/` - Debate skill using these templates
