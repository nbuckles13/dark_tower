// Real OTel JS SDK writers (the exact packages/versions sdk-core pins), DELTA
// temporality, OTLP/HTTP-proto, driven with controlled flushes so ordering is
// deterministic. Resource is the post-GC-filter shape (GC strips
// service.namespace / deployment.environment). Run from packages/sdk-core so
// module resolution picks up sdk-core's own node_modules.
//
// usage: node sdk-writers.mjs <otlp-base> <prom-base> <mode: two|idle|cumulative>
import { MeterProvider, PeriodicExportingMetricReader } from '@opentelemetry/sdk-metrics';
import { OTLPMetricExporter } from '@opentelemetry/exporter-metrics-otlp-proto';
import { AggregationTemporality as AggregationTemporalityPreference } from '@opentelemetry/sdk-metrics';
import { resourceFromAttributes } from '@opentelemetry/resources';

const [OTLP, PROM, MODE] = process.argv.slice(2);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function writer(temporality) {
  const exporter = new OTLPMetricExporter({
    url: `${OTLP}/v1/metrics`,
    temporalityPreference: temporality,
  });
  const reader = new PeriodicExportingMetricReader({ exporter, exportIntervalMillis: 3_600_000 });
  const mp = new MeterProvider({
    resource: resourceFromAttributes({
      'service.name': 'darktower-sdk-core',
      'service.version': '0.0.0',
    }),
    readers: [reader],
  });
  const meter = mp.getMeter('darktower-sdk-core', '0.0.0');
  return {
    mp,
    ctr: meter.createCounter('dt_client_media_frames_received_total'),
    hist: meter.createHistogram('dt_client_time_to_first_media_frame_ms'),
    flush: () => mp.forceFlush(),
  };
}

async function read(org) {
  const t = await (await fetch(`${PROM}/metrics`)).text();
  const pick = (series) => {
    const line = t
      .split('\n')
      .find((l) => l.startsWith(series + '{') && l.includes(`org_id="${org}"`));
    return line ? Number(line.trim().split(/\s+/).pop()) : null;
  };
  return {
    c: pick('dt_client_media_frames_received_total'),
    n: pick('dt_client_time_to_first_media_frame_ms_count'),
    s: pick('dt_client_time_to_first_media_frame_ms_sum'),
  };
}

const fails = [];
const check = (what, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  console.log(
    `    ${ok ? 'PASS' : 'FAIL'} ${what}: got ${JSON.stringify(got)} want ${JSON.stringify(want)}`,
  );
  if (!ok) fails.push(what);
};
const labels = (org) => ({ client_version: '0.0.0', org_id: org, key_custody: 'operator' });

if (MODE === 'two') {
  console.log('== SDK-S1 two real DELTA MeterProviders, same identity, interleaved flushes');
  const org = 'sdk-two';
  const A = writer(AggregationTemporalityPreference.DELTA);
  await sleep(1200); // B's meter starts later AND its first interval starts before A's first export lands
  const B = writer(AggregationTemporalityPreference.DELTA);
  let want = { c: 0, n: 0, s: 0 };
  for (let i = 1; i <= 3; i++) {
    for (const [name, w, frames, obs] of [
      ['A', A, 5, 200],
      ['B', B, 7, 700],
    ]) {
      w.ctr.add(frames, labels(org));
      w.hist.record(obs, labels(org));
      await sleep(1500);
      await w.flush();
      want = { c: want.c + frames, n: want.n + 1, s: want.s + obs };
      await sleep(800);
      const got = await read(org);
      console.log(
        `    ${name}${i}  counter=${got.c} count=${got.n} sum=${got.s}   want ${want.c}/${want.n}/${want.s}`,
      );
    }
  }
  check('SDK two-writer totals (counter/count/sum)', await read(org), { c: 36, n: 6, s: 2700 });
  await A.mp.shutdown();
  await B.mp.shutdown();
} else if (MODE === 'idle') {
  console.log('== SDK-S2 one real DELTA MeterProvider: 5, idle (flushes with nothing recorded), 6');
  const org = 'sdk-idle';
  const A = writer(AggregationTemporalityPreference.DELTA);
  A.ctr.add(5, labels(org));
  await sleep(1000);
  await A.flush();
  await sleep(800);
  console.log(`    after 5: ${(await read(org)).c}`);
  for (let i = 0; i < 4; i++) {
    await sleep(5000);
    await A.flush();
  } // periodic exports during idle: no datapoint
  A.ctr.add(6, labels(org));
  await sleep(1000);
  await A.flush();
  await sleep(800);
  const got = (await read(org)).c;
  console.log(`    after idle + 6: ${got}`);
  check('SDK idle single-writer counter', got, 11);
  await A.mp.shutdown();
} else if (MODE === 'restart') {
  console.log(
    '== SDK-S6 writer restart: a MeterProvider is shut down and a fresh one continues (a delta carries no reset)',
  );
  const org = 'sdk-restart';
  let W = writer(AggregationTemporalityPreference.DELTA);
  W.ctr.add(5, labels(org));
  await sleep(1000);
  await W.flush();
  await sleep(800);
  console.log(`    first writer: ${(await read(org)).c}`);
  await W.mp.shutdown();
  W = writer(AggregationTemporalityPreference.DELTA);
  W.ctr.add(7, labels(org));
  await sleep(1000);
  await W.flush();
  await sleep(800);
  const got = (await read(org)).c;
  console.log(`    after restart + 7: ${got}`);
  check('restarted writer keeps accumulating the fleet total', got, 12);
  await W.mp.shutdown();
} else if (MODE === 'cumulative') {
  console.log('== SDK-S5 a stale CUMULATIVE writer sharing the identity with a DELTA writer');
  const org = 'sdk-mixed';
  const D = writer(AggregationTemporalityPreference.DELTA);
  const C = writer(AggregationTemporalityPreference.CUMULATIVE);
  for (let i = 1; i <= 3; i++) {
    D.ctr.add(5, labels(org));
    await sleep(700);
    await D.flush();
    C.ctr.add(100, labels(org));
    await sleep(700);
    await C.flush();
    await sleep(800);
    console.log(`    round ${i}: counter=${(await read(org)).c} (delta-writer total ${5 * i})`);
  }
  check('cumulative writer excluded; delta total intact', (await read(org)).c, 15);
  await D.mp.shutdown();
  await C.mp.shutdown();
}
console.log('RESULT:', fails.length ? 'FAIL ' + fails.join(', ') : 'PASS');
process.exit(fails.length ? 1 : 0);
