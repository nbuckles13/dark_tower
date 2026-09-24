#!/usr/bin/env python3
"""Two-writer / idle / filtering acceptance driver for the OTel collector.

Emulates browsers AFTER the GC proxy filter: resource {service.name,
service.version}, scope darktower-sdk-core, datapoint attrs
{client_version, org_id, key_custody[, reason]}, DELTA temporality.
Posts OTLP/HTTP-JSON to :4318 and reads the prometheus exporter on :8889.

Exit 0 only if every assertion holds. Prints a table per scenario.
"""
import json, re, sys, time, urllib.request

OTLP = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:14318"
PROM = sys.argv[2] if len(sys.argv) > 2 else "http://127.0.0.1:18889"
CADENCE = float(sys.argv[3]) if len(sys.argv) > 3 else 4.0

DELTA = 1
BOUNDS = [0, 5, 10, 25, 50, 75, 100, 250, 500, 750, 1000, 2500, 5000, 7500, 10000]  # OTel JS default


def now_ns():
    return time.time_ns()


def attrs(d):
    return [{"key": k, "value": {"stringValue": v}} for k, v in d.items()]


def payload(metrics):
    return {
        "resourceMetrics": [{
            "resource": {"attributes": attrs({"service.name": "darktower-sdk-core", "service.version": "0.0.0"})},
            "scopeMetrics": [{"scope": {"name": "darktower-sdk-core", "version": "0.0.0"}, "metrics": metrics}],
        }]
    }


def counter(name, labels, value, start, end):
    return {"name": name, "sum": {"aggregationTemporality": DELTA, "isMonotonic": True, "dataPoints": [{
        "attributes": attrs(labels), "startTimeUnixNano": str(start), "timeUnixNano": str(end), "asInt": str(value)}]}}


def histogram(name, labels, observation, start, end):
    buckets = [0] * (len(BOUNDS) + 1)
    idx = next((i for i, b in enumerate(BOUNDS) if observation <= b), len(BOUNDS))
    buckets[idx] = 1
    return {"name": name, "histogram": {"aggregationTemporality": DELTA, "dataPoints": [{
        "attributes": attrs(labels), "startTimeUnixNano": str(start), "timeUnixNano": str(end),
        "count": "1", "sum": observation, "bucketCounts": [str(b) for b in buckets], "explicitBounds": BOUNDS}]}}


def post(metrics):
    req = urllib.request.Request(OTLP + "/v1/metrics", data=json.dumps(payload(metrics)).encode(),
                                 headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(req, timeout=10) as r:
        assert r.status == 200, r.status


def scrape():
    with urllib.request.urlopen(PROM + "/metrics", timeout=10) as r:
        return r.read().decode()


def value(text, series, must):
    """Value of the first sample of `series` whose label set contains every must=val."""
    for line in text.splitlines():
        if line.startswith("#"):
            continue
        m = re.match(r'^([a-zA-Z_:][a-zA-Z0-9_:]*)(\{(.*)\})?\s+(\S+)', line)
        if not m or m.group(1) != series:
            continue
        labels = dict(re.findall(r'([a-zA-Z_][a-zA-Z0-9_]*)="((?:[^"\\]|\\.)*)"', m.group(3) or ""))
        if all(labels.get(k) == v for k, v in must.items()):
            return float(m.group(4)), labels
    return None, None


def settle():
    time.sleep(1.0)


FAIL = []


def check(label, got, want):
    ok = got == want
    print(f"    {'PASS' if ok else 'FAIL'} {label}: got {got} want {want}")
    if not ok:
        FAIL.append(label)


CTR = "dt_client_media_frames_received_total"
HIST = "dt_client_time_to_first_media_frame_ms"
BASE = {"client_version": "0.0.0", "key_custody": "operator"}


def two_writer():
    print("== S1 two same-identity delta writers, out of phase (A: 5 frames + 200ms obs; B: 7 frames + 700ms obs)")
    lab = dict(BASE, org_id="two-writer")
    t0 = now_ns()
    last = {"A": t0, "B": t0 - int(CADENCE * 0.3e9)}
    want_c = want_n = 0
    want_s = 0.0
    print(f"    {'after':<6}{'counter':>9}{'h_count':>9}{'h_sum':>8}   {'want':>12}")
    for i in range(1, 4):
        for w, frames, obs in (("A", 5, 200.0), ("B", 7, 700.0)):
            time.sleep(CADENCE / 2)
            end = now_ns()
            post([counter(CTR, lab, frames, last[w], end), histogram(HIST, lab, obs, last[w], end)])
            last[w] = end
            want_c += frames; want_n += 1; want_s += obs
            settle()
            t = scrape()
            c, _ = value(t, CTR, {"org_id": "two-writer"})
            n, _ = value(t, HIST + "_count", {"org_id": "two-writer"})
            s, _ = value(t, HIST + "_sum", {"org_id": "two-writer"})
            print(f"    {w}{i:<5}{c!s:>9}{n!s:>9}{s!s:>8}   {want_c:>5} / {want_n} / {want_s:g}")
    check("two-writer counter", c, float(want_c))
    check("two-writer histogram _count", n, float(want_n))
    check("two-writer histogram _sum", s, want_s)


def idle(gap):
    print(f"== S2 single writer across an idle interval ({gap:g}s with no export): delta 5, idle, delta 6")
    lab = dict(BASE, org_id="idle")
    start = now_ns()
    time.sleep(CADENCE)
    end = now_ns()
    post([counter(CTR, lab, 5, start, end)])
    settle()
    c1, _ = value(scrape(), CTR, {"org_id": "idle"})
    print(f"    after delta 5: {c1}")
    time.sleep(gap)
    # Browser delta semantics: the next non-empty interval starts at the last
    # collection, so its start is AFTER the previous point's end (a gap), not equal.
    s2 = now_ns()
    time.sleep(CADENCE)
    post([counter(CTR, lab, 6, s2, now_ns())])
    settle()
    c2, _ = value(scrape(), CTR, {"org_id": "idle"})
    print(f"    after idle + delta 6: {c2}")
    check("idle single-writer counter", c2, 11.0)


def skew():
    """paired-client cases: writer clock skew, a late/retried export, a far-future point."""
    print("== S4 clock skew / late export / far-future point (same identity, 3 writers)")
    lab = dict(BASE, org_id="skew")
    S = int(30e9)
    want = 0
    steps = [("honest", 0, 2), ("slow -30s", -S, 3), ("fast +30s", S, 4), ("honest", 0, 2),
             ("late/retry (older than last)", -int(5e9), 5), ("far-future 2100", 4102444800 * 10**9 - now_ns(), 7),
             ("honest after future", 0, 2), ("slow -30s", -S, 3)]
    for what, off, n in steps:
        time.sleep(CADENCE / 2)
        end = now_ns() + off
        post([counter(CTR, lab, n, end - int(CADENCE * 1e9), end)])
        want += n
        settle()
        c, _ = value(scrape(), CTR, {"org_id": "skew"})
        print(f"    {what:<30} +{n}  -> {c}  (want {want})")
    check("skew/late/future all accumulate", c, float(want))


def stale(gap):
    print(f"== S7 stale reset (run with max_stale 20s override): delta 5, idle {gap:g}s > max_stale, delta 6")
    lab = dict(BASE, org_id="stale")
    s0 = now_ns(); time.sleep(CADENCE)
    post([counter(CTR, lab, 5, s0, now_ns())]); settle()
    print(f"    after 5: {value(scrape(), CTR, {'org_id': 'stale'})[0]}")
    time.sleep(gap)
    s1 = now_ns(); time.sleep(CADENCE)
    post([counter(CTR, lab, 6, s1, now_ns())]); settle()
    c, _ = value(scrape(), CTR, {"org_id": "stale"})
    print(f"    after stale gap + 6: {c}")
    check("stale stream restarts (visible reset, not summed through)", c, 6.0)


def filtering():
    print("== S3 metrics-path filtering controls")
    t0 = now_ns() - int(CADENCE * 1e9)
    t1 = now_ns()
    long_ver = "9" * 200
    post([
        counter("dt_client_not_allowlisted_total", dict(BASE, org_id="filt"), 1, t0, t1),
        counter("dt_client_media_frames_dropped_total",
                dict(BASE, org_id="filt", reason="no_kek_for_generation", meeting_id_hash="deadbeef"), 3, t0, t1),
        counter("dt_client_media_frames_sent_total", dict(BASE, org_id="filt-ver", client_version=long_ver), 2, t0, t1),
        counter("dt_client_media_frames_accepted_total", dict(BASE, org_id="filt-shape", client_version="1.2.3;DROP"), 2, t0, t1),
        counter("dt_client_media_key_wrap_outcomes_total", dict(BASE, org_id="filt-anchor", outcome="X truncated X"), 1, t0, t1),
        counter("dt_client_media_frames_received_total", dict(BASE, org_id="filt-nil"), 1, t0, t1),
        counter("dt_client_media_frames_dropped_total", dict(BASE, org_id="filt-real", reason="kek_generation_stale"), 1, t0, t1),
        counter("dt_client_media_send_dropped_total",
                dict(BASE, org_id="filt-forge", key_custody="end_to_end", reason="0f8fad5b-d9cb-469f-a165-70867728950e"), 4, t0, t1),
    ])
    settle()
    t = scrape()
    check("(a) unlisted name absent", "dt_client_not_allowlisted_total" in t, False)
    v, labels = value(t, "dt_client_media_frames_dropped_total", {"org_id": "filt"})
    check("(b) allowlisted name present with reason", (v, (labels or {}).get("reason")), (3.0, "no_kek_for_generation"))
    check("(b) meeting_id_hash stripped", "meeting_id_hash" in (labels or {}), False)
    check("(b) no identity label anywhere", "deadbeef" in t, False)
    _, labels = value(t, "dt_client_media_frames_sent_total", {"org_id": "filt-ver"})
    check("(c) over-length client_version rewritten", (labels or {}).get("client_version"), "invalid")
    _, labels = value(t, "dt_client_media_frames_accepted_total", {"org_id": "filt-shape"})
    check("(c) mis-shaped client_version rewritten", (labels or {}).get("client_version"), "invalid")
    _, labels = value(t, "dt_client_media_send_dropped_total", {"org_id": "filt-forge"})
    check("(B2) forged key_custody overwritten", (labels or {}).get("key_custody"), "operator")
    check("(B1) mis-shaped reason rewritten", (labels or {}).get("reason"), "invalid")
    _, labels = value(t, "dt_client_media_key_wrap_outcomes_total", {"org_id": "filt-anchor"})
    check("(anchor) legal substring + illegal surroundings rewritten", (labels or {}).get("outcome"), "invalid")
    _, labels = value(t, "dt_client_media_frames_received_total", {"org_id": "filt-nil"})
    check("(nil-guard) never-sent labels stay absent", sorted(k for k in (labels or {}) if k in ("reason", "outcome", "action", "source")), [])
    check("(real) client_version 0.0.0 survives", (labels or {}).get("client_version"), "0.0.0")
    _, labels = value(t, "dt_client_media_frames_dropped_total", {"org_id": "filt-real"})
    check("(real) future token kek_generation_stale survives", (labels or {}).get("reason"), "kek_generation_stale")
    check("(real) no_kek_for_generation survives", value(t, "dt_client_media_frames_dropped_total", {"org_id": "filt"})[1].get("reason"), "no_kek_for_generation")
    check("names verbatim (no unit suffix)", "dt_client_time_to_first_media_frame_ms_milliseconds" in t, False)
    check("names verbatim (no doubled _total)", "_total_total" in t, False)


if __name__ == "__main__":
    only = sys.argv[4] if len(sys.argv) > 4 else "all"
    if only in ("all", "two"):
        two_writer()
    if only in ("all", "idle"):
        idle(float(sys.argv[5]) if len(sys.argv) > 5 else 20.0)
    if only in ("all", "skew"):
        skew()
    if only == "stale":
        stale(float(sys.argv[5]) if len(sys.argv) > 5 else 40.0)
    if only in ("all", "filter"):
        filtering()
    print("RESULT:", "FAIL " + ", ".join(FAIL) if FAIL else "PASS")
    sys.exit(1 if FAIL else 0)
