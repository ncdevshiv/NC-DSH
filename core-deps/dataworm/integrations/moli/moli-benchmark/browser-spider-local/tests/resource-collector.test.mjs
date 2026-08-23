import assert from 'node:assert/strict';
import test from 'node:test';

import {
  LinuxProcessTreeCollector,
  parseProcStat,
  parseProcStatus,
  parseSmapsRollup
} from '../lib/observability/linux-procfs.mjs';
import { buildResourceArtifact } from '../lib/observability/sampler.mjs';

function processRecord({
  pid,
  ppid = 0,
  ticks = 0,
  start = pid * 10,
  threads = 1,
  rssPages = 1
}) {
  return {
    pid,
    ppid,
    state: 'S',
    cpu_ticks: ticks,
    start_time_ticks: start,
    thread_count: threads,
    rss_pages: rssPages
  };
}

class FakeProcfs {
  constructor(frames) {
    this.frames = frames;
    this.index = 0;
  }

  frame() {
    return this.frames[this.index];
  }

  advance() {
    this.index = Math.min(this.index + 1, this.frames.length - 1);
  }

  readStat(pid) {
    const process = this.frame().processes.get(pid);
    if (!process) {
      throw new Error(`missing pid ${pid}`);
    }
    return process;
  }

  scanProcesses() {
    return this.frame().processes;
  }

  readMemory(process) {
    return this.frame().memory.get(process.pid);
  }
}

function frame(processes, memory) {
  return {
    processes: new Map(processes.map((process) => [process.pid, process])),
    memory: new Map(Object.entries(memory).map(([pid, value]) => [Number(pid), value]))
  };
}

test('proc parsers preserve CPU, memory and thread semantics', () => {
  const fields = [
    'S', '7', '0', '0', '0', '0', '0', '0', '0', '0', '0',
    '120', '30', '0', '0', '0', '0', '9', '0', '444', '0', '12'
  ];
  const stat = parseProcStat(`42 (renderer worker) ${fields.join(' ')}`);
  assert.deepEqual(stat, {
    pid: 42,
    ppid: 7,
    state: 'S',
    cpu_ticks: 150,
    start_time_ticks: 444,
    thread_count: 9,
    rss_pages: 12
  });
  assert.deepEqual(
    parseProcStatus('VmRSS:\t321 kB\nThreads:\t7\n'),
    { rss_bytes: 321 * 1024, thread_count: 7 }
  );
  assert.equal(parseSmapsRollup('Rss: 400 kB\nPss: 123 kB\n'), 123 * 1024);
});

test('collector aggregates a process-tree delta and does not double-count overlapping roots', () => {
  const procfs = new FakeProcfs([
    frame(
      [
        processRecord({ pid: 10, ticks: 100, start: 1000, threads: 3 }),
        processRecord({ pid: 11, ppid: 10, ticks: 40, start: 1100, threads: 2 })
      ],
      {
        10: { rss_bytes: 1000, pss_bytes: 700, thread_count: 3 },
        11: { rss_bytes: 500, pss_bytes: 300, thread_count: 2 }
      }
    ),
    frame(
      [
        processRecord({ pid: 10, ticks: 120, start: 1000, threads: 3 }),
        processRecord({ pid: 11, ppid: 10, ticks: 50, start: 1100, threads: 2 })
      ],
      {
        10: { rss_bytes: 1200, pss_bytes: 800, thread_count: 3 },
        11: { rss_bytes: 600, pss_bytes: 350, thread_count: 2 }
      }
    )
  ]);
  const collector = new LinuxProcessTreeCollector({
    procfs,
    ticksPerSecond: 100,
    intervalMs: 100,
    clock: () => 0
  });
  collector.addRoot('browser', 10);
  collector.addRoot('renderer', 11);
  collector.sample({ elapsedMs: 0 });
  procfs.advance();
  const sample = collector.sample({ elapsedMs: 100 });

  assert.equal(sample.total.process_count, 2);
  assert.equal(sample.total.thread_count, 5);
  assert.equal(sample.total.rss_bytes, 1800);
  assert.equal(sample.total.pss_bytes, 1150);
  assert.ok(Math.abs(sample.total.cpu_percent - 300) < 1e-9);
  assert.equal(sample.workers.browser.process_count, 2);
  assert.equal(sample.workers.renderer.process_count, 1);
});

test('collector rejects recycled root PIDs and incomplete PSS', () => {
  const procfs = new FakeProcfs([
    frame(
      [
        processRecord({ pid: 10, ticks: 10, start: 1000 }),
        processRecord({ pid: 11, ppid: 10, ticks: 5, start: 1100 })
      ],
      {
        10: { rss_bytes: 1000, pss_bytes: 700, thread_count: 1 },
        11: { rss_bytes: 500, pss_bytes: null, thread_count: 1 }
      }
    ),
    frame(
      [processRecord({ pid: 10, ticks: 1, start: 9999 })],
      { 10: { rss_bytes: 200, pss_bytes: 150, thread_count: 1 } }
    )
  ]);
  const collector = new LinuxProcessTreeCollector({
    procfs,
    ticksPerSecond: 100,
    intervalMs: 100,
    clock: () => 0
  });
  collector.addRoot('browser', 10);
  const first = collector.sample({ elapsedMs: 0 });
  assert.equal(first.total.rss_bytes, 1500);
  assert.equal(first.total.pss_bytes, null);
  assert.equal(first.total.pss_process_count, 1);

  procfs.advance();
  const recycled = collector.sample({ elapsedMs: 100 });
  assert.equal(recycled.total.process_count, 0);
  assert.equal(recycled.workers.browser.process_count, 0);
});

test('resource artifact derives case, worker, weighted CPU and sampler health summaries', () => {
  const collector = {
    platform: 'linux',
    method: 'test',
    interval_ms: 100,
    cpu_ticks_per_second: 100,
    host_logical_cpu_count: 8,
    roots: {},
    errors: {},
    samples: [
      {
        elapsed_ms: 0,
        kind: 'root-registered',
        capture_duration_ms: 1,
        total: { cpu_percent: null, rss_bytes: 100, pss_bytes: 90, process_count: 1, thread_count: 2 },
        workers: { 'worker-1': { cpu_percent: null, rss_bytes: 100, pss_bytes: 90, process_count: 1, thread_count: 2 } }
      },
      {
        elapsed_ms: 100,
        kind: 'periodic',
        capture_duration_ms: 2,
        total: { cpu_percent: 50, rss_bytes: 200, pss_bytes: 180, process_count: 1, thread_count: 2 },
        workers: { 'worker-1': { cpu_percent: 50, rss_bytes: 200, pss_bytes: 180, process_count: 1, thread_count: 2 } }
      },
      {
        elapsed_ms: 200,
        kind: 'periodic',
        capture_duration_ms: 3,
        total: { cpu_percent: 75, rss_bytes: 250, pss_bytes: 230, process_count: 1, thread_count: 2 },
        workers: { 'worker-1': { cpu_percent: 75, rss_bytes: 250, pss_bytes: 230, process_count: 1, thread_count: 2 } }
      },
      {
        elapsed_ms: 300,
        kind: 'final',
        capture_duration_ms: 120,
        total: { cpu_percent: 100, rss_bytes: 300, pss_bytes: 280, process_count: 1, thread_count: 3 },
        workers: { 'worker-1': { cpu_percent: 100, rss_bytes: 300, pss_bytes: 280, process_count: 1, thread_count: 3 } }
      }
    ]
  };
  const artifact = buildResourceArtifact({
    enabled: true,
    status: 'available',
    intervalMs: 100,
    collector,
    markers: [
      { type: 'case-start', case_name: 'news', elapsed_ms: 50 },
      { type: 'case-done', case_name: 'news', elapsed_ms: 300 }
    ]
  });

  assert.equal(artifact.summary.peak_cpu_percent, 100);
  assert.equal(artifact.summary.average_cpu_percent, 75);
  assert.equal(artifact.summary.peak_rss_bytes, 300);
  assert.equal(artifact.summary.sampling_overrun_count, 1);
  assert.equal(artifact.summary.average_observed_interval_ms, 100);
  assert.equal(artifact.summary.max_observed_interval_ms, 100);
  assert.equal(artifact.summary.late_sample_count, 0);
  assert.equal(artifact.summary.workers['worker-1'].peak_pss_bytes, 280);
  assert.equal(artifact.summary.cases[0].case_name, 'news');
  assert.equal(artifact.summary.cases[0].sample_count, 3);
});
