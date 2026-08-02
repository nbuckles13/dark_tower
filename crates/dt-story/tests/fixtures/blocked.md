# User Story: Blocked

Task 2 is pending but its only dep escalated — `next` must exit 4.

```yaml
# task-metadata (dt-story manifest v1)
story: blocked-story
branch: feature/blocked-story
tasks:
- id: 1
  status: escalated
  escalation: escalations/task-1.log
- id: 2
  status: pending
  specialist: meeting-controller
  env_tests: true
  deps:
  - 1
  prompt: |
    Wire the session signal.
```
