# Architecture Decision Records (ADRs)

This directory contains Architecture Decision Records for the Dark Tower project.

## What is an ADR?

An Architecture Decision Record (ADR) captures an important architectural decision made along with its context and consequences. ADRs help future developers understand why certain design choices were made.

## ADR Format

Each ADR follows this structure:

```markdown
# ADR-NNNN: Title

**Status**: Proposed | Accepted | Deprecated | Superseded by ADR-XXXX

**Date**: YYYY-MM-DD

**Context**: What is the issue we're facing?

**Decision**: What did we decide to do?

**Consequences**: What are the positive and negative outcomes?

**Alternatives Considered**: What other options did we evaluate?

**References**: Links to relevant docs, discussions, or code
```

## Creating a New ADR

1. Copy the template: `cp adr-template.md adr-NNNN-short-title.md`
2. Fill in the ADR content
3. Update the index table above
4. Reference the ADR in relevant documentation (ARCHITECTURE.md, etc.)
5. Get review from specialist agents via debate if cross-cutting

## ADR Lifecycle

- **Proposed**: Under discussion, not yet implemented
- **Accepted**: Decision made, implementation in progress or complete
- **Deprecated**: No longer recommended, but not yet replaced
- **Superseded**: Replaced by a newer ADR

## Integration with Project Docs

ADRs complement other documentation:

- **ARCHITECTURE.md**: High-level system design (references ADRs for detailed decisions)
- **TECHNICAL_STACK.md**: Technology choices (references ADRs for rationale)
- **API_CONTRACTS.md**: Interface specifications (references ADRs for protocol decisions)
- **Specialist Agents**: Reference ADRs for patterns to follow

When making architectural decisions:
1. Check existing ADRs for precedent
2. Create new ADR for significant decisions
3. Update relevant documentation to reference the ADR
