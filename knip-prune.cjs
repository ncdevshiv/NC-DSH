const fs = require('fs');

// 1. knip.json: ignore the fixture consumer tree + drop auto-discovered workspace configs.
let k = fs.readFileSync('knip.json', 'utf8');
const igFrom = `  "ignoreWorkspaces": [
    "vendor/*",
    "python/sdk-runtime"
  ],`;
if (!k.includes(igFrom)) throw new Error('ignoreWorkspaces anchor missing');
k = k.replace(igFrom, `  "ignoreWorkspaces": [
    "vendor/*",
    "python/sdk-runtime",
    "packages/typert/generator/tests/fixtures/remote-model"
  ],`);
// Remove the two stale per-workspace config blocks (auto-discovered now).
for (const key of ['packages/util/home', 'packages/client/web-ui']) {
  const keyAt = k.indexOf(`"` + key + `": {`);
  if (keyAt < 0) throw new Error('workspace block missing: ' + key);
  const lineStart = k.lastIndexOf('\n', keyAt) + 1;
  let depth = 0;
  let i = k.indexOf('{', keyAt);
  for (; i < k.length; i++) {
    if (k[i] === '{') depth++;
    else if (k[i] === '}') { depth--; if (depth === 0) break; }
  }
  let end = k.indexOf('\n', i) + 1;
  // Drop a trailing comma on the removed block's closing line, or on the
  // previous line when this was the last entry of its parent object.
  let before = k.slice(0, lineStart);
  let after = k.slice(end);
  if (after.trimStart().startsWith(',')) after = after.slice(after.indexOf(',') + 1).replace(/^\n/, '');
  else if (/,\s*$/.test(before) === false && /,\n/.test(before)) {}
  if (/,\n?$/.test(before) === false) before = before.replace(/\n([^\n]*)$/, '\n$1'.replace(/,$/, '') + '');
  k = before.replace(/,(\s*)$/, '$1') + after;
}
JSON.parse(k);
fs.writeFileSync('knip.json', k);
console.log('knip.json updated');

// 2. coverage-partitions.ts: vitestInvocation is internal-only now.
let c = fs.readFileSync('scripts/coverage-partitions.ts', 'utf8');
c = c.replace('export function vitestInvocation(', 'function vitestInvocation(');
if (!c.includes('function vitestInvocation(')) throw new Error('un-export failed');
fs.writeFileSync('scripts/coverage-partitions.ts', c);
console.log('vitestInvocation un-exported');
