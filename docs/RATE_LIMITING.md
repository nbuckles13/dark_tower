# Rate Limiting Strategy

This document describes the hybrid rate limiting approach for the Dark Tower Auth Controller.

## Overview

The Auth Controller uses a **two-layer rate limiting strategy**:

1. **Nginx/Reverse Proxy Layer** - Fast, connection-level rate limiting
2. **Application Layer** - Account-based lockout for brute force protection

This hybrid approach provides both performance and security.

## Layer 1: Nginx Rate Limiting

### Purpose
- Prevent DoS/DDoS attacks
- Limit connections per IP address
- Fail fast before hitting the application

### Configuration

Add to your nginx configuration:

```nginx
# Define rate limit zones
http {
    # Zone for general API requests (10 req/sec per IP)
    limit_req_zone $binary_remote_addr zone=api_limit:10m rate=10r/s;

    # Zone for authentication endpoints (stricter - 3 req/sec per IP)
    limit_req_zone $binary_remote_addr zone=auth_limit:10m rate=3r/s;

    # Connection limit per IP
    limit_conn_zone $binary_remote_addr zone=conn_limit:10m;

    server {
        listen 443 ssl;
        server_name auth.dark-tower.example.com;

        # SSL configuration
        ssl_certificate /path/to/cert.pem;
        ssl_certificate_key /path/to/key.pem;

        # Global connection limit (max 10 concurrent connections per IP)
        limit_conn conn_limit 10;

        # Authentication endpoints (stricter limits)
        location ~ ^/api/v1/auth/(user|service)/token$ {
            limit_req zone=auth_limit burst=5 nodelay;
            limit_req_status 429;

            proxy_pass http://auth_controller_backend;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }

        # Admin endpoints (moderate limits)
        location /api/v1/admin/ {
            limit_req zone=api_limit burst=10 nodelay;
            limit_req_status 429;

            proxy_pass http://auth_controller_backend;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }

        # Public endpoints (JWKS, health)
        location / {
            limit_req zone=api_limit burst=20 nodelay;

            proxy_pass http://auth_controller_backend;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }
    }

    # Backend upstream
    upstream auth_controller_backend {
        server 127.0.0.1:8080;
        # Add more servers for load balancing
        # server 127.0.0.1:8081;
    }
}
```

### Rate Limit Parameters

| Zone | Rate | Burst | Use Case |
|------|------|-------|----------|
| `auth_limit` | 3 req/sec | 5 | Token endpoints (authentication) |
| `api_limit` | 10 req/sec | 10-20 | Admin and public endpoints |
| `conn_limit` | 10 connections | - | Maximum concurrent connections per IP |

### Custom Error Response

Add custom 429 error page:

```nginx
error_page 429 = @rate_limit_error;

location @rate_limit_error {
    default_type application/json;
    return 429 '{"error":{"code":"RATE_LIMIT_EXCEEDED","message":"Too many requests. Please try again later."}}';
}
```

## Layer 2: Application-Level Account Lockout

### Purpose
- Prevent credential brute-force attacks
- Protect individual accounts regardless of source IP
- Log suspicious activity for security monitoring

### Implementation

The Auth Controller implements account lockout in the token service:

```rust
// Check for account lockout (prevent brute force)
if let Some(ref cred) = credential {
    let fifteen_mins_ago = Utc::now() - chrono::Duration::minutes(15);
    let failed_count = auth_events::get_failed_attempts_count(
        pool,
        &cred.credential_id,
        fifteen_mins_ago
    ).await?;

    if failed_count >= 5 {
        tracing::warn!(
            "Account locked due to excessive failed attempts: client_id={}",
            client_id
        );
        return Err(AcError::RateLimitExceeded);
    }
}
```

### Parameters

- **Threshold**: 5 failed attempts
- **Window**: 15 minutes (rolling window)
- **Action**: Return `429 Too Many Requests` with `RATE_LIMIT_EXCEEDED` error code

### Lockout Behavior

1. Failed authentication attempts are logged in `auth_events` table
2. Before processing authentication, the service counts failed attempts in the last 15 minutes
3. If count >= 5, request is rejected immediately
4. After 15 minutes of no failed attempts, the account is automatically unlocked

### Database Query

```sql
SELECT COUNT(*)
FROM auth_events
WHERE credential_id = $1
  AND success = false
  AND created_at >= $2  -- 15 minutes ago
```

## Monitoring and Alerting

### Metrics to Track

1. **Rate Limit Hits**
   - Nginx 429 responses per endpoint
   - Application-level lockouts per credential

2. **Failed Authentication Attempts**
   - Total failed attempts per time window
   - Credentials with repeated failures

3. **IP Address Patterns**
   - IPs with high failure rates
   - Distributed brute force attempts (many IPs, same credential)

### Recommended Alerts

```yaml
# Example Prometheus alerting rules
groups:
  - name: auth_security
    rules:
      - alert: HighFailedAuthRate
        expr: rate(auth_events_failed_total[5m]) > 10
        for: 5m
        annotations:
          summary: "High rate of failed authentication attempts"

      - alert: AccountLockout
        expr: auth_account_lockouts_total > 0
        for: 1m
        annotations:
          summary: "Account lockout triggered - possible brute force"

      - alert: NginxRateLimitExceeded
        expr: rate(nginx_http_requests_total{status="429"}[5m]) > 50
        for: 5m
        annotations:
          summary: "High rate of nginx rate limit hits"
```

## Testing Rate Limits

### Test Nginx Rate Limiting

```bash
# Test auth endpoint rate limit (should get 429 after 3 req/sec)
for i in {1..10}; do
  curl -w "\n%{http_code}\n" \
    -X POST https://auth.example.com/api/v1/auth/service/token \
    -H "Content-Type: application/json" \
    -d '{"grant_type":"client_credentials","client_id":"test","client_secret":"test"}'
  sleep 0.1
done
```

### Test Application-Level Lockout

```bash
# Make 6 failed authentication attempts to trigger lockout
for i in {1..6}; do
  echo "Attempt $i"
  curl -w "\n%{http_code}\n" \
    -X POST https://auth.example.com/api/v1/auth/service/token \
    -H "Content-Type: application/json" \
    -d '{"grant_type":"client_credentials","client_id":"valid-client-id","client_secret":"WRONG_SECRET"}'
  sleep 1
done
```

Expected behavior:
- Attempts 1-5: Return 401 (Invalid Credentials)
- Attempt 6+: Return 429 (Rate Limit Exceeded)

## Security Considerations

### Strengths

1. **Defense in Depth**: Two layers provide redundancy
2. **Distributed Attack Protection**: Account lockout works across IPs
3. **Automatic Recovery**: Lockout window expires automatically
4. **Audit Trail**: All attempts logged in database

### Limitations

1. **Distributed Brute Force**: Attackers using many IPs may still attempt credential stuffing
2. **Legitimate Lockouts**: Configuration errors or testing may lock out legitimate services
3. **Database Load**: Counting failed attempts adds database queries

### Mitigations

1. **Monitor**: Set up alerting for unusual patterns
2. **Manual Override**: Provide admin API to unlock accounts manually (future enhancement)
3. **Adjust Parameters**: Tune threshold and window based on actual usage patterns
4. **IP Reputation**: Consider integrating IP reputation services for additional protection

## Future Enhancements

1. **Adaptive Rate Limiting**: Adjust limits based on threat level
2. **CAPTCHA Integration**: Challenge suspicious requests before lockout
3. **Distributed Rate Limiting**: Use Redis for rate limit state across multiple instances
4. **Geographic Restrictions**: Block or challenge requests from unexpected regions
5. **Manual Account Unlock API**: Allow admins to unlock accounts before window expires

## References

- [OWASP: Blocking Brute Force Attacks](https://owasp.org/www-community/controls/Blocking_Brute_Force_Attacks)
- [Nginx Rate Limiting](https://www.nginx.com/blog/rate-limiting-nginx/)
- [OAuth 2.0 Security Best Current Practice](https://datatracker.ietf.org/doc/html/draft-ietf-oauth-security-topics)
