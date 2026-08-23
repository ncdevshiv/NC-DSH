import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '..', '..');
const CLOCK_TICKS_PER_SECOND = Number.parseInt(
  execFileSync('getconf', ['CLK_TCK'], { encoding: 'utf8' }).trim(),
  10
);
if (!Number.isFinite(CLOCK_TICKS_PER_SECOND) || CLOCK_TICKS_PER_SECOND <= 0) {
  throw new Error(`invalid CLK_TCK: ${CLOCK_TICKS_PER_SECOND}`);
}

function parseArgs(argv) {
  const args = {
    binary: path.join(REPO_ROOT, 'target', 'release', 'moli'),
    outputDir: null,
    navigations: 120,
    temporaryPageEvery: 12,
    payloadObjects: 24000,
    domNodes: 4000,
    domSource: 'script',
    promises: 1000,
    externalScriptBytes: 0,
    extraction: 'none',
    navigationDelayMs: 0,
    settleMs: 5000,
    expectModel: 'page-vm'
  };
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    const next = () => {
      index += 1;
      if (index >= argv.length) {
        throw new Error(`missing value after ${value}`);
      }
      return argv[index];
    };
    if (value === '--binary') {
      args.binary = path.resolve(next());
    } else if (value === '--output-dir') {
      args.outputDir = path.resolve(next());
    } else if (value === '--navigations') {
      args.navigations = Number.parseInt(next(), 10);
    } else if (value === '--temporary-page-every') {
      args.temporaryPageEvery = Number.parseInt(next(), 10);
    } else if (value === '--payload-objects') {
      args.payloadObjects = Number.parseInt(next(), 10);
    } else if (value === '--dom-nodes') {
      args.domNodes = Number.parseInt(next(), 10);
    } else if (value === '--dom-source') {
      args.domSource = next();
    } else if (value === '--promises') {
      args.promises = Number.parseInt(next(), 10);
    } else if (value === '--external-script-bytes') {
      args.externalScriptBytes = Number.parseInt(next(), 10);
    } else if (value === '--extraction') {
      args.extraction = next();
    } else if (value === '--navigation-delay-ms') {
      args.navigationDelayMs = Number.parseInt(next(), 10);
    } else if (value === '--settle-ms') {
      args.settleMs = Number.parseInt(next(), 10);
    } else if (value === '--expect-model') {
      args.expectModel = next();
    } else {
      throw new Error(`unknown argument: ${value}`);
    }
  }
  if (!args.outputDir) {
    throw new Error('--output-dir is required');
  }
  if (!Number.isInteger(args.navigations) || args.navigations < 1) {
    throw new Error('--navigations must be a positive integer');
  }
  if (!Number.isInteger(args.temporaryPageEvery) || args.temporaryPageEvery < 0) {
    throw new Error('--temporary-page-every must be a non-negative integer');
  }
  for (const key of [
    'payloadObjects',
    'domNodes',
    'promises',
    'externalScriptBytes',
    'navigationDelayMs',
    'settleMs'
  ]) {
    if (!Number.isInteger(args[key]) || args[key] < 0) {
      throw new Error(`--${key.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`)} must be a non-negative integer`);
    }
  }
  if (!['script', 'markup'].includes(args.domSource)) {
    throw new Error('--dom-source must be script or markup');
  }
  if (!['none', 'content', 'selectors', 'full'].includes(args.extraction)) {
    throw new Error('--extraction must be none, content, selectors, or full');
  }
  return args;
}

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function readProcessSample(pid) {
  const rawStat = fs.readFileSync(`/proc/${pid}/stat`, 'utf8');
  const close = rawStat.lastIndexOf(')');
  const fields = rawStat.slice(close + 2).trim().split(/\s+/);
  const ticks = Number(fields[11]) + Number(fields[12]);
  const status = fs.readFileSync(`/proc/${pid}/status`, 'utf8');
  const statusMiB = (key) => {
    const match = new RegExp(`^${key}:\\s+(\\d+)\\s+kB$`, 'm').exec(status);
    return match ? Number(match[1]) / 1024 : null;
  };
  return {
    ticks,
    rssMiB: statusMiB('VmRSS'),
    rssAnonMiB: statusMiB('RssAnon'),
    rssFileMiB: statusMiB('RssFile'),
    vmDataMiB: statusMiB('VmData'),
    vmSwapMiB: statusMiB('VmSwap')
  };
}

function linearSlope(values) {
  if (values.length < 2) {
    return null;
  }
  const meanX = (values.length - 1) / 2;
  const meanY = values.reduce((sum, value) => sum + value, 0) / values.length;
  let numerator = 0;
  let denominator = 0;
  for (let index = 0; index < values.length; index += 1) {
    numerator += (index - meanX) * (values[index] - meanY);
    denominator += (index - meanX) ** 2;
  }
  return denominator === 0 ? null : numerator / denominator;
}

function average(values) {
  return values.length === 0
    ? null
    : values.reduce((sum, value) => sum + value, 0) / values.length;
}

function isolateCounters(diagnostics) {
  const isolateScope = diagnostics.isolateScope;
  const accounting = isolateScope?.documentIsolateAccounting;
  const activeBrowserContext = diagnostics.activeBrowserContext;
  const runtimeSession = activeBrowserContext?.runtimeSession;
  if (!isolateScope) {
    throw new Error(`missing isolate diagnostics: ${JSON.stringify(diagnostics)}`);
  }
  return {
    model: isolateScope.documentIsolateModel,
    created: accounting?.created ?? null,
    destroyed: accounting?.destroyed ?? null,
    live: accounting?.live ?? null,
    reserved: accounting?.reserved ?? null,
    loadedDocumentPageCount: isolateScope.loadedDocumentPageCount,
    estimatedDocumentIsolateCount: isolateScope.estimatedDocumentIsolateCount,
    documentContextCount: isolateScope.documentContextCount ?? null,
    isolatedWorldContextCount: isolateScope.isolatedWorldContextCount ?? null,
    childDefaultContextCount: isolateScope.childDefaultContextCount ?? null,
    dedicatedWorkerLoadingCount: isolateScope.dedicatedWorkerLoadingCount ?? null,
    dedicatedWorkerRunningWorkerIsolateCount:
      isolateScope.dedicatedWorkerRunningWorkerIsolateCount ?? null,
    estimatedWorkerIsolateCount: isolateScope.estimatedWorkerIsolateCount,
    estimatedLiveV8IsolateCount: isolateScope.estimatedLiveV8IsolateCount ?? null,
    pendingInspectorAwaitCount: isolateScope.pendingInspectorAwaitCount ?? null,
    rendererInspectorSessionRetained:
      runtimeSession?.rendererInspectorSessionRetained ?? null,
    activeLoadedPage: activeBrowserContext?.activeLoadedPage ?? null,
    domRemoteObjectNodeCacheCount:
      activeBrowserContext?.domRemoteObjectNodeCacheCount ?? null
  };
}

function pageHtml(index, payload) {
  const markupRows = payload.domSource === 'markup'
    ? Array.from(
      { length: payload.domNodes },
      (_, nodeIndex) => `<a class="row" data-bucket="${nodeIndex % 10}" href="/item/${index}/${nodeIndex}">page-${index}-node-${nodeIndex}</a>`
    ).join('')
    : '';
  const scriptedDomNodes = payload.domSource === 'script' ? payload.domNodes : 0;
  const externalScript = payload.externalScriptBytes > 0
    ? `<script src="/script.js?index=${index}"></script>`
    : '';
  return `<!doctype html>
<meta charset="utf-8">
<title>isolate-sequence-${index}</title>
<style>
  .row:nth-child(3n) { color: rgb(20, 40, 60); }
  .row[data-bucket="5"] { contain: layout style; }
</style>
<main id="root">${markupRows}</main>
<script>
  const marker = ${JSON.stringify(`page-${index}-`)};
  globalThis.__retainedPayload = Array.from({ length: ${payload.payloadObjects} }, (_, itemIndex) => ({
    itemIndex,
    text: (marker + itemIndex).padEnd(112, "x"),
    values: [itemIndex, itemIndex + 1, itemIndex + 2, itemIndex + 3]
  }));
  const fragment = document.createDocumentFragment();
  for (let nodeIndex = 0; nodeIndex < ${scriptedDomNodes}; nodeIndex += 1) {
    const row = document.createElement("a");
    row.className = "row";
    row.dataset.bucket = String(nodeIndex % 10);
    row.href = "/item/" + ${index} + "/" + nodeIndex;
    row.textContent = marker + "node-" + nodeIndex;
    fragment.appendChild(row);
  }
  document.getElementById("root").appendChild(fragment);
  globalThis.__resolvedPromises = Array.from({ length: ${payload.promises} }, (_, promiseIndex) =>
    Promise.resolve(marker + promiseIndex)
  );
</script>
${externalScript}`;
}

function externalScriptBody(index, requestedBytes) {
  const prefix = `globalThis.__externalScriptMarker=${JSON.stringify(`external-${index}`)};/*`;
  const suffix = '*/';
  return `${prefix}${'x'.repeat(Math.max(0, requestedBytes - prefix.length - suffix.length))}${suffix}`;
}

async function extractSpiderSurface(page, mode) {
  const result = {
    htmlLength: null,
    locatorCount: null,
    sampledItemCount: 0
  };
  if (mode === 'content' || mode === 'full') {
    result.htmlLength = (await page.content()).length;
  }
  if (mode === 'selectors' || mode === 'full') {
    const elements = await page.locator('a.row').all();
    result.locatorCount = elements.length;
    for (const element of elements.slice(0, 5)) {
      const text = await element.innerText();
      const href = await element.getAttribute('href');
      if (text && href) {
        result.sampledItemCount += 1;
      }
    }
  }
  return result;
}

function startFixture(payload) {
  const server = http.createServer((request, response) => {
    const url = new URL(request.url || '/', 'http://127.0.0.1');
    const index = Number.parseInt(url.searchParams.get('index') || '0', 10);
    if (url.pathname === '/script.js') {
      const body = externalScriptBody(
        Number.isFinite(index) ? index : 0,
        payload.externalScriptBytes
      );
      response.writeHead(200, {
        'cache-control': 'public, max-age=3600',
        'content-length': Buffer.byteLength(body),
        'content-type': 'text/javascript; charset=utf-8'
      });
      response.end(body);
      return;
    }
    const body = pageHtml(Number.isFinite(index) ? index : 0, payload);
    response.writeHead(200, {
      'cache-control': 'no-store',
      'content-length': Buffer.byteLength(body),
      'content-type': 'text/html; charset=utf-8'
    });
    response.end(body);
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      resolve({
        baseUrl: `http://127.0.0.1:${address.port}`,
        close: () => new Promise((closeResolve) => server.close(closeResolve))
      });
    });
  });
}

function startMoli(binary, logs) {
  const child = spawn(binary, ['serve', '--host', '127.0.0.1', '--port', '0'], {
    cwd: REPO_ROOT,
    detached: true,
    env: { ...process.env, NO_PROXY: '*', no_proxy: '*' },
    stdio: ['ignore', 'pipe', 'pipe']
  });
  const address = new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('timed out waiting for Moli address')), 15000);
    const onChunk = (chunk) => {
      const text = chunk.toString('utf8');
      logs.push(text);
      const match = /protocol server listening addr=(127\.0\.0\.1:\d+)/.exec(text);
      if (match) {
        clearTimeout(timeout);
        resolve(match[1]);
      }
    };
    child.stdout.on('data', onChunk);
    child.stderr.on('data', onChunk);
    child.once('error', (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    child.once('exit', (code, signal) => {
      clearTimeout(timeout);
      reject(new Error(`Moli exited before startup: code=${code} signal=${signal}`));
    });
  });
  return { child, address };
}

async function stopMoli(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) {
    return;
  }
  try {
    process.kill(-child.pid, 'SIGTERM');
  } catch (_error) {
    child.kill('SIGTERM');
  }
  await Promise.race([
    new Promise((resolve) => child.once('exit', resolve)),
    new Promise((resolve) => setTimeout(resolve, 2000))
  ]);
  if (child.exitCode === null && child.signalCode === null) {
    try {
      process.kill(-child.pid, 'SIGKILL');
    } catch (_error) {
      child.kill('SIGKILL');
    }
  }
}

async function browserWebSocketUrl(address) {
  const response = await fetch(`http://${address}/json/version`);
  if (!response.ok) {
    throw new Error(`/json/version returned ${response.status}`);
  }
  const payload = await response.json();
  return payload.webSocketDebuggerUrl;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (fs.existsSync(args.outputDir)) {
    throw new Error(`refusing to reuse output directory: ${args.outputDir}`);
  }
  fs.mkdirSync(args.outputDir, { recursive: true });
  if (!fs.existsSync(args.binary)) {
    throw new Error(`binary does not exist: ${args.binary}`);
  }

  const fixture = await startFixture(args);
  const logs = [];
  const moli = startMoli(args.binary, logs);
  let browser;
  const records = [];
  const startedAt = process.hrtime.bigint();
  const processStart = readProcessSample(moli.child.pid);
  try {
    const address = await moli.address;
    browser = await chromium.connectOverCDP(await browserWebSocketUrl(address), { timeout: 15000 });
    const browserSession = await browser.newBrowserCDPSession();
    const context = browser.contexts()[0] || await browser.newContext();
    const page = context.pages()[0] || await context.newPage();
    const pageSession = await context.newCDPSession(page);
    const baselineDiagnostics = await browserSession.send('HeapProfiler.moliDiagnostics');
    const baselineCounters = isolateCounters(baselineDiagnostics);

    for (let index = 1; index <= args.navigations; index += 1) {
      await page.goto(`${fixture.baseUrl}/page?index=${index}`, {
        waitUntil: 'domcontentloaded',
        timeout: 30000
      });
      if (args.externalScriptBytes > 0) {
        const marker = await page.evaluate(() => globalThis.__externalScriptMarker);
        if (marker !== `external-${index}`) {
          throw new Error(`external script did not execute for navigation ${index}: ${marker}`);
        }
      }
      const extraction = await extractSpiderSurface(page, args.extraction);
      const [heap, diagnostics] = await Promise.all([
        pageSession.send('Runtime.getHeapUsage'),
        browserSession.send('HeapProfiler.moliDiagnostics')
      ]);
      records.push({
        kind: 'navigation',
        index,
        ...readProcessSample(moli.child.pid),
        usedHeapMiB: heap.usedSize / 1024 / 1024,
        totalHeapMiB: heap.totalSize / 1024 / 1024,
        embedderHeapUsedMiB: (heap.embedderHeapUsedSize ?? 0) / 1024 / 1024,
        backingStorageMiB: heap.backingStorageSize / 1024 / 1024,
        networkMemoryCache:
          diagnostics.connection?.activeNavigationEngine?.networkMemoryCache ?? null,
        extraction,
        counters: isolateCounters(diagnostics)
      });

      if (args.temporaryPageEvery > 0 && index % args.temporaryPageEvery === 0) {
        const temporaryPage = await context.newPage();
        await temporaryPage.goto(`${fixture.baseUrl}/temporary?index=${index}`, {
          waitUntil: 'domcontentloaded',
          timeout: 30000
        });
        const beforeClose = isolateCounters(
          await browserSession.send('HeapProfiler.moliDiagnostics')
        );
        await temporaryPage.close();
        const afterClose = isolateCounters(
          await browserSession.send('HeapProfiler.moliDiagnostics')
        );
        records.push({
          kind: 'temporary-page-close',
          index,
          ...readProcessSample(moli.child.pid),
          beforeClose,
          afterClose
        });
      }
      if (args.navigationDelayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, args.navigationDelayMs));
      }
    }

    const beforeFinalCloseDiagnostics = await browserSession.send(
      'HeapProfiler.moliDiagnostics'
    );
    const beforeFinalClose = isolateCounters(beforeFinalCloseDiagnostics);
    await page.close();
    let afterFinalClose;
    let afterFinalCloseDiagnostics;
    for (let attempt = 0; attempt < 40; attempt += 1) {
      afterFinalCloseDiagnostics = await browserSession.send(
        'HeapProfiler.moliDiagnostics'
      );
      afterFinalClose = isolateCounters(afterFinalCloseDiagnostics);
      if (
        afterFinalClose.live === null ||
        (afterFinalClose.live === 0 && afterFinalClose.reserved === 0)
      ) {
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    const postCloseSamples = [];
    const settleStartedAt = Date.now();
    do {
      postCloseSamples.push({
        elapsedMs: Date.now() - settleStartedAt,
        ...readProcessSample(moli.child.pid)
      });
      if (Date.now() - settleStartedAt >= args.settleMs) {
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    } while (true);

    const navigationRecords = records.filter((record) => record.kind === 'navigation');
    const rssValues = navigationRecords.map((record) => record.rssMiB);
    const heapValues = navigationRecords.map((record) => record.usedHeapMiB);
    const warmStart = Math.floor(navigationRecords.length / 2);
    const warmRssValues = rssValues.slice(warmStart);
    const warmHeapValues = heapValues.slice(warmStart);
    const networkMemoryCacheRecords = navigationRecords
      .map((record) => record.networkMemoryCache)
      .filter((value) => value !== null);
    const firstWindow = rssValues.slice(0, Math.min(10, rssValues.length));
    const lastWindow = rssValues.slice(-Math.min(10, rssValues.length));
    const processEnd = readProcessSample(moli.child.pid);
    const elapsedSeconds = Number(process.hrtime.bigint() - startedAt) / 1e9;
    const summary = {
      binary: args.binary,
      binarySha256: execFileSync('sha256sum', [args.binary], { encoding: 'utf8' })
        .trim()
        .split(/\s+/)[0],
      navigations: args.navigations,
      temporaryPageEvery: args.temporaryPageEvery,
      payloadObjects: args.payloadObjects,
      domNodes: args.domNodes,
      domSource: args.domSource,
      promises: args.promises,
      externalScriptBytes: args.externalScriptBytes,
      extraction: args.extraction,
      navigationDelayMs: args.navigationDelayMs,
      settleMs: args.settleMs,
      elapsedSeconds,
      childUserPlusSystemCpuSeconds:
        (processEnd.ticks - processStart.ticks) / CLOCK_TICKS_PER_SECOND,
      peakRssMiB: Math.max(...rssValues),
      firstTenAverageRssMiB: average(firstWindow),
      lastTenAverageRssMiB: average(lastWindow),
      firstToLastWindowRssMiB: average(lastWindow) - average(firstWindow),
      warmHalfRssSlopeMiBPerNavigation: linearSlope(warmRssValues),
      peakCurrentIsolateUsedHeapMiB: Math.max(...heapValues),
      warmHalfUsedHeapSlopeMiBPerNavigation: linearSlope(warmHeapValues),
      peakNetworkMemoryCacheRetainedMiB: networkMemoryCacheRecords.length === 0
        ? null
        : Math.max(...networkMemoryCacheRecords.map((value) => value.retainedBytes)) / 1024 / 1024,
      finalNetworkMemoryCache: networkMemoryCacheRecords.at(-1) ?? null,
      postCloseInitialRssMiB: postCloseSamples.at(0)?.rssMiB ?? null,
      postCloseFinalRssMiB: postCloseSamples.at(-1)?.rssMiB ?? null,
      postCloseMinimumRssMiB: Math.min(...postCloseSamples.map((sample) => sample.rssMiB)),
      postCloseFinalAnonRssMiB: postCloseSamples.at(-1)?.rssAnonMiB ?? null,
      postCloseFinalVmDataMiB: postCloseSamples.at(-1)?.vmDataMiB ?? null,
      postCloseFinalVmSwapMiB: postCloseSamples.at(-1)?.vmSwapMiB ?? null,
      baselineCounters,
      beforeFinalClose,
      afterFinalClose,
      createdDelta: afterFinalClose.created === null
        ? null
        : afterFinalClose.created - baselineCounters.created,
      destroyedDelta: afterFinalClose.destroyed === null
        ? null
        : afterFinalClose.destroyed - baselineCounters.destroyed,
      baselineOutstandingIsolates: baselineCounters.created === null
        ? null
        : baselineCounters.created - baselineCounters.destroyed,
      finalOutstandingIsolates: afterFinalClose.created === null
        ? null
        : afterFinalClose.created - afterFinalClose.destroyed
    };
    if (baselineCounters.model !== args.expectModel) {
      throw new Error(`unexpected document isolate model: ${baselineCounters.model}`);
    }
    if (args.expectModel === 'page-vm') {
      if (afterFinalClose.live !== 0 || afterFinalClose.reserved !== 0) {
        throw new Error(`document isolate ownership did not close: ${JSON.stringify(afterFinalClose)}`);
      }
      if (summary.baselineOutstandingIsolates !== baselineCounters.live) {
        throw new Error(`baseline isolate accounting mismatch: ${JSON.stringify(summary)}`);
      }
      if (summary.finalOutstandingIsolates !== afterFinalClose.live) {
        throw new Error(`final isolate accounting mismatch: ${JSON.stringify(summary)}`);
      }
    }
    writeJson(path.join(args.outputDir, 'records.json'), records);
    writeJson(path.join(args.outputDir, 'post-close-samples.json'), postCloseSamples);
    writeJson(path.join(args.outputDir, 'diagnostics.json'), {
      baseline: baselineDiagnostics,
      beforeFinalClose: beforeFinalCloseDiagnostics,
      afterFinalClose: afterFinalCloseDiagnostics
    });
    writeJson(path.join(args.outputDir, 'summary.json'), summary);
    fs.writeFileSync(path.join(args.outputDir, 'moli.log'), logs.join(''), 'utf8');
    console.log(JSON.stringify(summary, null, 2));
  } finally {
    await browser?.close().catch(() => undefined);
    await stopMoli(moli.child);
    await fixture.close();
  }
}

main().catch((error) => {
  console.error(error.stack || error.message || String(error));
  process.exit(1);
});
