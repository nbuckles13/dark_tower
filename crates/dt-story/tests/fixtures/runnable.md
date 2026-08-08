# User Story: Client Joins Meeting

As a client I want to join a meeting so that I can collaborate.

## Prose Before The Manifest

This paragraph, the headings, and everything after the manifest block must
survive `complete`/`escalate` byte-for-byte.

```yaml
# task-metadata (dt-story manifest v1)
story: client-join-meeting
branch: feature/client-join-meeting
tasks:
- id: 1
  status: completed
  commit: abc1230
- id: 2
  status: pending
  specialist: global-controller
  env_tests: true
  deps:
  - 1
  prompt: |
    Implement the join endpoint.
    Return a meeting token on success.
- id: 3
  status: pending
  specialist: test
  env_tests: false
  deps:
  - 2
  prompt: |
    Add e2e coverage for the join flow.
```

## Prose After The Manifest

Trailing content — also preserved exactly.

```bash
echo "an unrelated fenced block that must not confuse the parser"
```
