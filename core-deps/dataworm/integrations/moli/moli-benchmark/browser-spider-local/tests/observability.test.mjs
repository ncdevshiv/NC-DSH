import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { SpiderRunObserver } from '../lib/observability/index.mjs';

function args() {
  return {
    targets: ['moli'],
    workers: 1,
    parallelism: 1,
    runs: 1,
    cases: ['news'],
    siteLimit: 1,
    siteSource: 'fixture',
    gotoTimeoutMs: 1500,
    sampleResources: false,
    sampleIntervalMs: 500
  };
}

test('public observer owns service artifacts and the final dashboard lifecycle', async () => {
  const runDir = fs.mkdtempSync(path.join(os.tmpdir(), 'spider-observer-test-'));
  try {
    const outputDir = path.join(runDir, 'output-moli');
    fs.mkdirSync(outputDir);
    const runObserver = new SpiderRunObserver({ runDir, args: args() });
    const serviceObserver = runObserver.beginService({
      outputDir,
      service: 'moli',
      target: 'moli'
    });
    serviceObserver.registerWorker('worker-1', process.pid);
    serviceObserver.mark({ type: 'case-start', caseName: 'news' });
    serviceObserver.mark({ type: 'case-done', caseName: 'news' });
    const resourceData = await serviceObserver.finish();
    const report = runObserver.writeReport([{
      target: 'moli',
      service: 'moli',
      success: true,
      outputDir,
      report: { summary: { averageFillRate: 100 }, files: [] },
      leakage: { suspiciousCount: 0 },
      resourceData
    }]);

    assert.equal(resourceData.status, 'disabled');
    assert.ok(fs.existsSync(path.join(outputDir, 'resource-samples.json')));
    assert.ok(fs.existsSync(path.join(outputDir, 'resource-samples.csv')));
    assert.ok(fs.existsSync(report.htmlPath));
    assert.ok(fs.existsSync(report.dataPath));
  } finally {
    fs.rmSync(runDir, { recursive: true, force: true });
  }
});

test('run observer refuses duplicate services and reports from unfinished services', async () => {
  const runDir = fs.mkdtempSync(path.join(os.tmpdir(), 'spider-observer-state-test-'));
  try {
    const outputDir = path.join(runDir, 'output-moli');
    fs.mkdirSync(outputDir);
    const runObserver = new SpiderRunObserver({ runDir, args: args() });
    const serviceObserver = runObserver.beginService({
      outputDir,
      service: 'moli',
      target: 'moli'
    });
    assert.throws(
      () => runObserver.beginService({ outputDir, service: 'moli', target: 'moli' }),
      /already registered/
    );
    assert.throws(() => runObserver.writeReport([]), /before observability finishes/);
    await serviceObserver.finish();
  } finally {
    fs.rmSync(runDir, { recursive: true, force: true });
  }
});
