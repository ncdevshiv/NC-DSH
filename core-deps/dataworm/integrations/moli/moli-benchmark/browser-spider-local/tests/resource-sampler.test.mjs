import assert from 'node:assert/strict';
import test from 'node:test';

import { ProcessTreeResourceSampler } from '../lib/observability/sampler.mjs';

test('worker-thread sampler captures the current Linux process tree', {
  skip: process.platform !== 'linux',
  timeout: 5000
}, async () => {
  const sampler = new ProcessTreeResourceSampler({ intervalMs: 100 });
  sampler.start();
  sampler.addRoot('self', process.pid);
  sampler.mark({ type: 'case-start', caseName: 'fixture' });
  await new Promise((resolve) => setTimeout(resolve, 240));
  sampler.mark({ type: 'case-done', caseName: 'fixture' });
  const artifact = await sampler.stop();

  assert.equal(artifact.status, 'available');
  assert.ok(artifact.samples.length >= 3);
  assert.ok(artifact.summary.peak_rss_bytes > 0);
  assert.ok(artifact.summary.peak_process_count >= 1);
  assert.equal(artifact.summary.cases[0].case_name, 'fixture');
  assert.ok(
    artifact.samples.at(-1).elapsed_ms
      >= artifact.markers.find((marker) => marker.type === 'case-done').elapsed_ms
  );
});

test('disabled sampler produces an explicit empty artifact', async () => {
  const sampler = new ProcessTreeResourceSampler({ enabled: false });
  sampler.start();
  sampler.mark({ type: 'service-start', service: 'moli' });
  const artifact = await sampler.stop();

  assert.equal(artifact.status, 'disabled');
  assert.equal(artifact.enabled, false);
  assert.equal(artifact.samples.length, 0);
  assert.equal(artifact.markers[0].service, 'moli');
});
