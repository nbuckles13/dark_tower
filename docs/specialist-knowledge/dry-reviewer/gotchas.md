# DRY Reviewer - Gotchas to Avoid

## Common Pitfalls

### 1. Over-Blocking on Established Tech Debt
**Pitfall**: Blocking new features because they duplicate existing patterns

**Why it happens**: Natural tendency to enforce DRY rigorously without distinguishing between new and existing duplication

**How to avoid**:
- Always check integration.md before marking as BLOCKER
- If pattern exists elsewhere in codebase, classify as TECH_DEBT instead
- Only block on NEW duplication patterns
- Cite ADR-0019 in reasoning

**Example**: JWT signing duplication (TD-1) - found in both ac-service and user provisioning:
- ✅ CORRECT: Classify as TECH_DEBT, document as TD-1
- ❌ WRONG: Block the implementation because signing logic is duplicated

### 2. Extraction Without Refactoring First
**Pitfall**: Recommending extraction of duplicated code without planning first

**Why it happens**: DRY principle pressure, under-estimating integration complexity

**How to avoid**:
- For TECH_DEBT: Don't recommend extraction in code review
- Document in integration.md with improvement strategy
- Let architectural refactoring be planned separately
- Reference in .claude/TODO.md if planned for future phase

**Example**: Key loading duplication (TD-2) spans multiple services:
- ✅ CORRECT: Document as tech debt, plan extraction for future phase
- ❌ WRONG: Request extraction in code review (blocks progress)

### 3. Ignoring Context of New Code
**Pitfall**: Treating duplication in new code as equivalent to duplication in mature code

**Why it happens**: Pattern matching without considering maturity and stability

**How to avoid**:
- Consider age/stability of duplicated code
- New implementations may stabilize differently than established patterns
- Give new code time to mature before forcing extraction
- Document expected convergence point

### 4. Scope Creep in Code Review
**Pitfall**: Using duplication review to refactor adjacent code

**Why it happens**: Natural desire to improve overall code health

**How to avoid**:
- Stay focused on duplication in the changeset
- Don't recommend changes outside scope
- Document adjacent duplication in integration.md
- Reference in tech debt for future work

## Security Considerations

### DRY Reviews Must Not Reduce Security
- Never compromise on security checks to reduce duplication
- If duplication involves security code, coordinate with Security specialist
- Duplicate security code is safer than insecure shortcuts
- Reference any security duplication in integration.md separately

## Integration Gotchas

### Cross-Service Pattern Recognition
- **Gotcha**: Assuming similar code in different services is duplication
- **Reality**: Services may have independent implementations due to different constraints
- **Solution**: Check commit history and comments before classifying as duplication

### Trait vs Duplication
- **Gotcha**: Trait extraction adds complexity without proportional benefit
- **Reality**: Rust trait bounds can become harder to maintain than duplicated code
- **Solution**: Only recommend trait extraction for 3+ similar implementations

