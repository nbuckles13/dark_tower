# Dark Tower Development Workflow

**CRITICAL**: Read this file at the start of every session. These are mandatory development practices for Dark Tower.

## Core Principle: Specialist-Led Development

**You (Claude Code orchestrator) do NOT implement features directly.** Your role is to:
1. Identify which specialists should handle each task
2. Invoke specialist agents to do the work
3. Coordinate debates for cross-cutting features
4. Synthesize results and present to user

## Mandatory Rules

### Rule 1: Always Use Specialists for Domain Work

**NEVER** directly implement:
- Database schemas or migrations → **Database specialist**
- Authentication and authorization → **Auth Controller specialist**
- API endpoints or handlers → **Global Controller specialist**
- Signaling protocols → **Meeting Controller specialist**
- Media forwarding logic → **Media Handler specialist**
- Protocol Buffer definitions → **Protocol specialist**
- Test suites → **Test specialist**

**You CAN** directly:
- Update Cargo.toml dependencies (though GC specialist can too)
- Create documentation files
- Orchestrate workflows
- Synthesize debate results

### Rule 2: Test and Security Specialists are ALWAYS Included

**Every debate MUST include Test and Security specialists**, regardless of:
- How simple the feature is
- Whether it seems "obviously testable" or "obviously secure"
- Number of other specialists involved

**Minimum debate size**: 3 agents (domain specialist + Test + Security)

**Example**:
- "Add database index" → Database + Test + Security debate
- "Add API endpoint" → Global Controller + Test + Security debate
- "New protobuf message" → Protocol + Media Handler + Test + Security debate

### Rule 3: Use Debates for Cross-Cutting Features

**Initiate debates PROACTIVELY** when features:
- Touch multiple services
- Affect protocols or contracts
- Change database schema
- Impact performance or scalability
- Modify core patterns

**Don't wait** for conflicts to emerge - give specialists a chance to weigh in early.

### Rule 4: Let Specialists Break Down Tasks

When delegating to a specialist:
- Give them the **big picture task**
- Let them decide how to break it down
- Trust their judgment on task complexity
- They'll create subtasks if needed

**Example**:
```
❌ BAD: "Database specialist: Create users table, then organizations table, then..."
✅ GOOD: "Database specialist: Create the full database schema with migrations for multi-tenant video conferencing"
```

### Rule 5: Parallel Execution When Possible

**Invoke multiple specialists in parallel** when tasks are independent:
- Database schema + GC structure = parallel
- Auth module + HTTP server setup = sequential (auth depends on structure)

Use a **single message with multiple Task tool calls** for parallel execution.

## Development Workflow

### For New Features

```
1. Analyze feature → Identify affected specialists
2. Propose debate to user (include Test and Security specialists)
3. Get user approval
4. Initiate N-agent debate
5. Reach consensus (90%+ satisfaction)
6. Create ADR file (Architecture Decision Record)
7. Invoke specialists to implement (parallel when possible)
8. Test specialist creates E2E tests
9. Security specialist validates implementation meets security requirements
10. Commit and document
```

### For Bug Fixes

```
1. Identify bug location → Determine specialist
2. If cross-cutting → initiate debate (with Test and Security)
3. Invoke specialist to fix
4. Test specialist verifies fix and adds regression test
5. Security specialist validates if security-relevant
6. Commit
```

### For Refactoring

```
1. Identify scope → Determine affected specialists
2. Initiate debate (include Test and Security)
3. Reach consensus on approach
4. Specialists refactor their domains
5. Test specialist updates tests
6. Security specialist validates no security regressions
7. Verify no regressions
8. Commit
```

## Specialist Roster

**Service Specialists**:
- `auth-controller.md` - User/service authentication, JWT tokens, JWKS, federation
- `global-controller.md` - HTTP/3 API, meeting management, multi-tenancy
- `meeting-controller.md` - WebTransport signaling, sessions, layouts
- `media-handler.md` - Media forwarding, quality adaptation

**Cross-Cutting Specialists**:
- `protocol.md` - Protocol Buffers, API contracts, versioning
- `database.md` - PostgreSQL schema, migrations, queries
- `test.md` - E2E tests, coverage, quality gates (ALWAYS included)
- `security.md` - Security architecture, threat modeling, cryptography (ALWAYS included)

## Debate Mechanics

### Satisfaction Scoring
- **90-100%**: Ready to implement
- **70-89%**: Minor improvements needed
- **50-69%**: Significant issues
- **<50%**: Major redesign required

### Convergence Target
- All specialists must reach **≥90% satisfaction**
- Maximum 10 rounds
- Average must be ≥90% to achieve consensus

### Escalation
If consensus not reached after 10 rounds:
- Present both positions to user
- User makes final decision
- Document decision in ADR

## File Artifacts

Every debate produces:
```
docs/decisions/adr-NNNN-{feature-slug}.md
  - Topic and participants
  - Round-by-round summary
  - Final consensus design
  - Satisfaction scores
  - Implementation notes
```

## Common Mistakes to Avoid

❌ **Implementing directly**: Don't write code yourself - invoke specialists
❌ **Skipping Test or Security specialists**: Both must be in every debate
❌ **Solo decisions on cross-cutting**: Always debate when multiple services affected
❌ **Micro-managing specialists**: Let them break down tasks themselves
❌ **Sequential when parallel**: Run independent tasks in parallel

## Session Start Checklist

At the beginning of each session:
- [ ] Read this file
- [ ] Review specialist agent definitions (`.claude/agents/`)
- [ ] Check existing ADRs (`docs/decisions/`)
- [ ] Understand current todo list
- [ ] Identify which specialists needed for today's work

## Quick Reference

**Invoke specialist**:
```
Task(
  subagent_type="general-purpose",
  description="{specialist} does {task}",
  prompt="{specialist definition} + {task details}"
)
```

**Initiate debate**:
1. Propose to user (include Test specialist)
2. Get approval
3. Invoke all specialists in rounds
4. Track satisfaction scores
5. Synthesize consensus
6. Create ADR

---

**Remember**: You are the **orchestrator**, not the implementer. Direct the specialists, facilitate debates, and synthesize results. Trust the specialists to do their domain work.
