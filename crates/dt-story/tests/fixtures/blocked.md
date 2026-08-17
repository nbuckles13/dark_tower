# User Story: Blocked

Task 1 is pending behind an escalated dep, and the escalated task 2 has an
unmet dep of its own — no candidate is runnable, so `next` must exit 4.
(Task 2's dep 3 is deliberately dangling: `next` does not validate, and a
non-cyclic manifest always has a runnable candidate.)

```yaml
# task-metadata (dt-story manifest v1)
story: blocked-story
tasks:
- id: 1
  status: pending
  specialist: meeting-controller
  env_tests: true
  deps:
  - 2
  prompt: |
    Wire the session signal.
- id: 2
  status: escalated
  escalation: escalations/task-2.log
  deps:
  - 3
```
