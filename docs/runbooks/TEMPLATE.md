# Runbook: [Alert Name]

**Alert**: [Alert rule name from Prometheus - e.g., GCHighLatency]
**Severity**: [Critical | Warning]
**Service**: [Service Name - e.g., Global Controller]
**Owner**: [Team Name - e.g., SRE Team]
**Last Updated**: [YYYY-MM-DD]

---

## Symptom

[What the alert indicates - the observable problem]

**Example**: "P95 latency for HTTP requests exceeds 200ms for more than 5 minutes"

---

## Impact

**User Impact**:
- [How this affects end users - e.g., "Slow page loads", "Timeouts during meeting join"]

**Business Impact**:
- [How this affects business metrics/SLOs - e.g., "SLO violation: 99.9% availability target at risk"]

**Blast Radius**:
- [What percentage of users/requests are affected - e.g., "All users in us-west region"]

---

## Diagnosis

Follow these steps to identify the root cause:

### Step 1: [Check High-Level Metrics]

```promql
# [PromQL query description - e.g., "Check current p95 latency by endpoint"]
[actual query - e.g., histogram_quantile(0.95, rate(gc_http_request_duration_seconds_bucket[5m]))]
```

**Expected Output**: [What healthy state looks like - e.g., "All endpoints < 200ms"]
**If Abnormal**: [What to do next - e.g., "Proceed to Step 2 to check database latency"]

### Step 2: [Check Specific Component]

```bash
# [Command description - e.g., "Check pod status and resource usage"]
kubectl get pods -n [namespace] -l app=[service]
kubectl top pods -n [namespace] -l app=[service]
```

**Expected Output**: [What to look for - e.g., "All pods Running, CPU < 80%, Memory < 85%"]
**If Abnormal**: [Go to specific scenario - e.g., "If CPU > 80%, go to Scenario A"]

### Step 3: [Dive Deeper]

[Continue with numbered steps - check logs, database queries, external dependencies, etc.]

---

## Mitigation

### Scenario A: [Specific Cause - e.g., "High CPU Usage"]

**Symptoms**:
- [How to identify this scenario - e.g., "CPU > 80% sustained, p95 latency increasing"]

**Immediate Actions**:

1. **[Action with expected result - e.g., "Scale up replicas"]**
   ```bash
   kubectl scale deployment [service] --replicas=[N] -n [namespace]
   ```
   **Expected Recovery Time**: [X minutes - e.g., "2-3 minutes for new pods to become ready"]

2. **[Next action - e.g., "Verify recovery"]**
   ```bash
   # Check that latency is decreasing
   [PromQL query or kubectl command]
   ```

**Long-term Fixes**:
- [Preventive measures - e.g., "Configure HPA to auto-scale based on CPU", "Optimize hot code paths identified in profile"]

### Scenario B: [Another Cause - e.g., "Database Slow Queries"]

**Symptoms**:
- [How to identify this scenario]

**Immediate Actions**:
1. [Action 1]
2. [Action 2]

**Long-term Fixes**:
- [Preventive measures]

### Scenario C: [Add more as needed]

[Repeat pattern for additional root causes]

---

## Example Queries

### Useful PromQL Queries

```promql
# [Query description - e.g., "Current error rate by endpoint"]
[query - e.g., rate(gc_http_requests_total{status=~"5.."}[5m])]
```

```promql
# [Another query]
[query]
```

### Useful kubectl Commands

```bash
# [Command description - e.g., "Get recent pod logs"]
kubectl logs -n [namespace] -l app=[service] --tail=100 --since=10m
```

```bash
# [Command description - e.g., "Check recent events"]
kubectl get events -n [namespace] --sort-by='.lastTimestamp' | tail -20
```

### Useful Database Queries

```sql
-- [Query description - e.g., "Find slow queries in last 10 minutes"]
SELECT query, mean_exec_time, calls
FROM pg_stat_statements
WHERE mean_exec_time > 100
ORDER BY mean_exec_time DESC
LIMIT 10;
```

---

## Escalation

**Escalate to**: [Team Name - e.g., Database Team, Platform Team, Development Team]

**When**: [Conditions for escalation]
- [Condition 1 - e.g., "No improvement after 30 minutes of mitigation"]
- [Condition 2 - e.g., "Root cause requires database schema changes"]
- [Condition 3 - e.g., "Issue affects multiple services"]

**Contact**:
- **Primary**: [Slack channel - e.g., #incidents]
- **Secondary**: [PagerDuty group or specific person]
- **Oncall**: [How to find current oncall - e.g., "Use /oncall command in #incidents"]

**Information to Provide When Escalating**:
- Alert fired at: [Timestamp from Alertmanager]
- Current [metric value]: [From diagnosis queries]
- Actions already taken: [List mitigation steps attempted]
- Recent deployments:
  ```bash
  kubectl rollout history deployment/[service] -n [namespace]
  ```
- Relevant logs: [Link to log aggregator with pre-filtered query]

---

## Post-Incident

After resolving the incident, complete these steps:

### 1. Document in #incidents Channel

Post incident summary:
```
🔴 INCIDENT RESOLVED

Service: [Service Name]
Alert: [Alert Name]
Fired At: [HH:MM UTC]
Resolved At: [HH:MM UTC]
Duration: [X minutes]

Root Cause: [Brief description]
TTD (Time to Detect): [X minutes from issue start to alert]
TTR (Time to Resolve): [X minutes from alert to resolution]

Actions Taken:
- [Action 1]
- [Action 2]

Impact:
- [User impact description]
- [SLO impact - e.g., "Consumed 30min of error budget"]
```

### 2. Update This Runbook

If during the incident you discovered:
- **New scenario**: Add it to Mitigation section
- **Commands didn't work**: Update or replace them
- **Missing information**: Add queries, commands, or context
- **Better approach**: Update mitigation steps

Make these updates **within 24 hours** while the incident is fresh.

### 3. File Follow-up Tasks

Create tickets/issues for:
- **Long-term fixes**: [e.g., "Optimize slow query identified in incident"]
- **Process improvements**: [e.g., "Add alert for leading indicator metric"]
- **Monitoring gaps**: [e.g., "Add dashboard panel for X metric"]
- **Infrastructure improvements**: [e.g., "Enable connection pooling"]

Tag with: `post-incident`, `[service-name]`, `[alert-name]`

### 4. Schedule Retrospective (for Critical Severity Only)

Within 3 business days, schedule a blameless retrospective:

**Attendees**:
- Incident responder(s)
- Service owner
- Relevant team members

**Agenda**:
- Timeline walkthrough
- What went well
- What could be improved
- Action items with owners

**Output**: Document in `docs/incidents/YYYY-MM-DD-[alert-name].md`

---

## Related Runbooks

- [Link to related runbook 1 - e.g., docs/runbooks/gc-database-issues.md]
- [Link to related runbook 2]
- [Link to infrastructure runbook - e.g., docs/runbooks/kubernetes-pod-crashes.md]

---

## Changelog

| Date | Author | Changes |
|------|--------|---------|
| YYYY-MM-DD | [Your Name] | Initial creation from TEMPLATE.md |

---

**Template Version**: 1.0
**Template Source**: `docs/runbooks/TEMPLATE.md`
