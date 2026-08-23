import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import vm from 'node:vm';

import {
  spiderReportDocument,
  writeSpiderReport
} from '../lib/observability/dashboard.mjs';
import {
  buildSpiderReportData,
  buildSiteDiagnostics,
  resourceSamplesCsv,
  writeResourceArtifacts
} from '../lib/observability/report-data.mjs';

function fixtureResourceData() {
  return {
    schema: 'moli.browser-spider.resources.v1',
    enabled: true,
    status: 'available',
    error: null,
    sampling: {
      interval_ms: 100,
      method: 'test',
      host_logical_cpu_count: 8,
      errors: {}
    },
    markers: [
      { type: 'case-start', case_name: 'news', elapsed_ms: 0 },
      {
        type: 'site-start',
        case_name: 'news',
        site: 'fixture news',
        worker: 'worker-1',
        elapsed_ms: 10
      },
      {
        type: 'site-done',
        case_name: 'news',
        site: 'fixture news',
        worker: 'worker-1',
        elapsed_ms: 90,
        success: true,
        item_count: 5
      },
      { type: 'case-done', case_name: 'news', elapsed_ms: 100 }
    ],
    samples: [
      {
        elapsed_ms: 100,
        wall_time: '2026-07-26T00:00:00.000Z',
        kind: 'periodic',
        capture_duration_ms: 1.25,
        total: {
          cpu_percent: 88,
          rss_bytes: 2048,
          rss_process_count: 1,
          pss_bytes: 1024,
          pss_process_count: 1,
          process_count: 1,
          thread_count: 4
        },
        workers: {}
      }
    ],
    summary: {
      sample_count: 1,
      peak_cpu_percent: 88,
      average_cpu_percent: 88,
      peak_rss_bytes: 2048,
      peak_pss_bytes: 1024,
      peak_process_count: 1,
      peak_thread_count: 4,
      average_capture_duration_ms: 1.25,
      max_capture_duration_ms: 1.25,
      sampling_overrun_count: 0,
      workers: {},
      cases: []
    }
  };
}

function fixtureArgs() {
  return {
    targets: ['moli'],
    workers: 1,
    parallelism: 1,
    runs: 1,
    cases: ['news'],
    siteLimit: 1,
    siteSource: 'fixture',
    gotoTimeoutMs: 1500,
    sampleResources: true,
    sampleIntervalMs: 100
  };
}

function fixtureMetadata() {
  return {
    cases: [{
      caseName: 'news',
      siteMeta: [{
        caseName: 'news',
        site: 'fixture news',
        url: 'https://example.test/news',
        gotoOk: false,
        gotoError: 'page.goto: Timeout 1500ms exceeded. | at runSite',
        responseStatus: null,
        finalUrlAfterExtract: 'https://example.test/news',
        title: 'Fixture news',
        htmlLength: 0,
        htmlSha256: null,
        htmlSaveError: 'page.content: document changed during snapshot. | at runSite',
        expectedItemCount: 5,
        itemCount: 5,
        itemClassification: 'expected',
        items: [{
          title: 'Fixture item',
          link: 'https://example.test/news/item'
        }]
      }]
    }]
  };
}

test('resource CSV preserves explicit process-tree fields', () => {
  const csv = resourceSamplesCsv(fixtureResourceData());
  assert.match(csv, /^elapsed_ms,wall_time,kind,cpu_percent,rss_bytes,rss_process_count,pss_bytes,/);
  assert.match(csv, /100,2026-07-26T00:00:00.000Z,periodic,88,2048,1,1024,1,1,4,1.25/);
});

test('report payload and document retain charts, raw links and safely embedded data', () => {
  const runDir = '/tmp/spider-report';
  const outputDir = `${runDir}/output-moli`;
  const payload = buildSpiderReportData({
    runDir,
    args: fixtureArgs(),
    results: [{
      target: 'moli',
      service: '</script><script>alert(1)</script>',
      success: true,
      outputDir,
      report: {
        summary: {
          totalActualRows: 5,
          totalExpectedRows: 5,
          averageFillRate: 100
        },
        files: []
      },
      leakage: { suspiciousCount: 0, mismatchedItemSites: 0, timeoutWithItems: 0 },
      metadata: fixtureMetadata(),
      resourceData: fixtureResourceData()
    }]
  });
  const document = spiderReportDocument(payload);

  assert.equal(payload.config.sample_interval_ms, 100);
  assert.match(document, /Peak resource comparison/);
  assert.match(document, /Memory timeline · process tree/);
  assert.match(document, /Case duration and extraction quality/);
  assert.match(document, /Slowest site phases · top 20/);
  assert.match(document, /Site result distribution/);
  assert.match(document, /HTML snapshot warnings/);
  assert.match(document, /resource-samples\.csv/);
  assert.match(document, /phaseBands/);
  assert.equal(payload.services[0].sites[0].outcome, 'timeout_with_items');
  assert.equal(payload.services[0].sites[0].duration_ms, 80);
  assert.equal(payload.services[0].sites[0].snapshot_saved, false);
  assert.equal(
    payload.services[0].sites[0].snapshot_error_summary,
    'page.content: document changed during snapshot.'
  );
  assert.equal(document.includes('</script><script>alert(1)</script>'), false);
  assert.ok(document.includes('\\u003c/script>'));
  const inlineScripts = [...document.matchAll(/<script(?: [^>]*)?>([\s\S]*?)<\/script>/g)]
    .map((match) => match[1])
    .filter((script) => script.trim() && !script.trim().startsWith('{'));
  assert.equal(inlineScripts.length, 1);
  assert.doesNotThrow(() => new vm.Script(inlineScripts[0]));
});

test('report and resource artifacts are generated as standalone files', () => {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'spider-report-test-'));
  try {
    const outputDir = path.join(tempDir, 'output-moli');
    fs.mkdirSync(outputDir);
    const resourceData = fixtureResourceData();
    writeResourceArtifacts(outputDir, resourceData);
    const result = {
      target: 'moli',
      service: 'moli',
      success: true,
      outputDir,
      report: { summary: { averageFillRate: 100 }, files: [] },
      leakage: { suspiciousCount: 0 },
      metadata: fixtureMetadata(),
      resourceData
    };
    const written = writeSpiderReport({
      runDir: tempDir,
      args: fixtureArgs(),
      results: [result]
    });

    assert.ok(fs.existsSync(path.join(outputDir, 'resource-samples.json')));
    assert.ok(fs.existsSync(path.join(outputDir, 'resource-samples.csv')));
    assert.ok(fs.existsSync(written.htmlPath));
    assert.ok(fs.existsSync(written.dataPath));
    assert.equal(
      JSON.parse(fs.readFileSync(written.dataPath, 'utf8')).schema,
      'moli.browser-spider.report.v3'
    );
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test('site diagnostics retain timing and classify navigation outcomes', () => {
  const sites = buildSiteDiagnostics({
    metadata: fixtureMetadata(),
    resourceData: fixtureResourceData()
  });

  assert.deepEqual(sites.map((site) => ({
    case_name: site.case_name,
    site: site.site,
    duration_ms: site.duration_ms,
    outcome: site.outcome,
    expected_item_count: site.expected_item_count,
    item_count: site.item_count,
    error_summary: site.error_summary,
    snapshot_saved: site.snapshot_saved,
    snapshot_error_summary: site.snapshot_error_summary
  })), [{
    case_name: 'news',
    site: 'fixture news',
    duration_ms: 80,
    outcome: 'timeout_with_items',
    expected_item_count: 5,
    item_count: 5,
    error_summary: 'page.goto: Timeout 1500ms exceeded.',
    snapshot_saved: false,
    snapshot_error_summary: 'page.content: document changed during snapshot.'
  }]);
});

test('site diagnostics distinguish snapshot failures from ordinary empty pages', () => {
  const metadata = fixtureMetadata();
  const site = metadata.cases[0].siteMeta[0];
  site.gotoOk = true;
  site.gotoError = null;
  site.itemCount = 0;
  site.items = [];

  const [diagnostic] = buildSiteDiagnostics({
    metadata,
    resourceData: fixtureResourceData()
  });

  assert.equal(diagnostic.outcome, 'snapshot_error_empty');
  assert.equal(diagnostic.expected_item_count, 5);
});
