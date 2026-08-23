from __future__ import annotations

import datetime as dt
import json
from pathlib import Path
from typing import Any

from .artifacts import write_json, write_text


REPORT_DATA_SCHEMA = "moli.agent-episode.report.v1"


def _embedded_json(payload: dict[str, Any]) -> str:
    return (
        json.dumps(payload, ensure_ascii=False, separators=(",", ":"))
        .replace("</", "<\\/")
        .replace("\u2028", "\\u2028")
        .replace("\u2029", "\\u2029")
    )


def write_agent_episode_report(
    *,
    suite_dir: Path,
    summary: dict[str, Any],
    episode_rows: list[dict[str, Any]],
    step_rows: list[dict[str, Any]],
    resources: dict[str, Any],
    markers: list[dict[str, Any]],
    config: dict[str, Any],
) -> dict[str, Any]:
    payload = {
        "schema": REPORT_DATA_SCHEMA,
        "generated_at": dt.datetime.now(dt.UTC).isoformat(),
        "summary": summary,
        "config": config,
        "episodes": episode_rows,
        "steps": step_rows,
        "resources": resources,
        "phase_markers": markers,
    }
    write_json(suite_dir / "report-data.json", payload)
    write_text(suite_dir / "index.html", _render_html(payload))
    return payload


def _render_html(payload: dict[str, Any]) -> str:
    embedded = _embedded_json(payload)
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Agent Episode CDP Benchmark</title>
<style>
:root {{ color-scheme: dark; --bg:#091019; --panel:#111c28; --line:#26384a;
  --text:#e7edf4; --muted:#91a3b5; --good:#4dd4a0; --bad:#ff6b7a;
  --blue:#61a8ff; --violet:#b28cff; --amber:#f4c15d; }}
* {{ box-sizing:border-box; }}
body {{ margin:0; background:var(--bg); color:var(--text); font:14px/1.45 ui-sans-serif,system-ui,sans-serif; }}
main {{ width:min(1500px,96vw); margin:0 auto; padding:32px 0 60px; }}
h1,h2,h3 {{ margin:0 0 12px; letter-spacing:-.02em; }} h1 {{ font-size:30px; }} h2 {{ margin-top:30px; }}
.subtitle,.muted {{ color:var(--muted); }} .subtitle {{ margin:4px 0 22px; }}
.grid {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(190px,1fr)); gap:12px; }}
.card,.panel {{ background:var(--panel); border:1px solid var(--line); border-radius:10px; }}
.card {{ padding:16px; }} .card .value {{ display:block; font-size:25px; font-weight:700; margin-top:5px; }}
.panel {{ padding:18px; margin-top:12px; overflow:auto; }}
.good {{ color:var(--good); }} .bad {{ color:var(--bad); }}
table {{ width:100%; border-collapse:collapse; font-variant-numeric:tabular-nums; }}
th,td {{ text-align:left; padding:8px 10px; border-bottom:1px solid var(--line); vertical-align:top; }}
th {{ color:var(--muted); font-size:12px; text-transform:uppercase; letter-spacing:.04em; }}
code {{ color:#c6ddf4; }} a {{ color:var(--blue); }}
.charts {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(430px,1fr)); gap:12px; }}
.chart {{ min-height:275px; }} svg {{ width:100%; height:225px; overflow:visible; }}
.axis {{ stroke:#43566a; stroke-width:1; }} .gridline {{ stroke:#203243; stroke-width:1; }}
.rss {{ stroke:var(--blue); }} .pss {{ stroke:var(--violet); }} .cpu {{ stroke:var(--amber); }}
.series {{ fill:none; stroke-width:2; vector-effect:non-scaling-stroke; }}
.marker {{ stroke:#718399; stroke-width:1; stroke-dasharray:2 4; opacity:.5; }}
.legend {{ display:flex; gap:18px; color:var(--muted); font-size:12px; }}
.swatch {{ display:inline-block; width:16px; border-top:2px solid; margin-right:5px; vertical-align:middle; }}
.nowrap {{ white-space:nowrap; }} .error {{ max-width:540px; white-space:pre-wrap; overflow-wrap:anywhere; }}
.empty {{ color:var(--muted); padding:16px 0; }}
</style>
</head>
<body><main>
<h1>Agent Episode CDP Benchmark</h1>
<p class="subtitle">Deterministic local RL-shaped CDP episodes. Correctness gates latency and resource interpretation.</p>
<section id="kpis" class="grid"></section>
<h2>Target summary</h2><section id="targets" class="panel"></section>
<h2>Correct operation latency</h2><section id="operations" class="panel"></section>
<h2>Resource timelines</h2><section id="charts" class="charts"></section>
<h2>Case correctness</h2><section id="cases" class="panel"></section>
<h2>Failure drilldown</h2><section id="failures" class="panel"></section>
<h2>Episode rows</h2><section id="episodes" class="panel"></section>
<h2>Artifact contract</h2>
<section class="panel"><code>report-data.json</code> is the renderer-independent authority.
Raw rows: <a href="runs.csv">runs.csv</a>, <a href="steps.csv">steps.csv</a>;
resources: <a href="resource-samples.json">resource-samples.json</a>;
phase log: <a href="events.log">events.log</a>.</section>
<script id="report-data" type="application/json">{embedded}</script>
<script>
const data = JSON.parse(document.getElementById('report-data').textContent);
const summary = data.summary;
const esc = value => String(value ?? '').replace(/[&<>"']/g, c => ({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[c]));
const num = (value, digits=1) => value == null || !Number.isFinite(Number(value)) ? '—' : Number(value).toFixed(digits);
const ms = value => value == null ? '—' : `${{num(value)}} ms`;
const mib = value => value == null ? '—' : `${{num(Number(value)/(1024*1024))}} MiB`;
const pct = (ok,total) => total ? `${{num(100*ok/total)}}%` : '—';
const median = stat => stat && stat.p50 != null ? stat.p50 : null;
function table(headers, rows) {{
  if (!rows.length) return '<div class="empty">No rows.</div>';
  return `<table><thead><tr>${{headers.map(h=>`<th>${{esc(h)}}</th>`).join('')}}</tr></thead><tbody>${{
    rows.map(row=>`<tr>${{row.map(cell=>`<td>${{cell}}</td>`).join('')}}</tr>`).join('')
  }}</tbody></table>`;
}}
const kpis = [
  ['Episodes', summary.episodes_total], ['Passed', summary.episodes_passed],
  ['Failed', summary.total_failures], ['Steps observed', summary.steps_total],
  ['Assertions', `${{summary.assertions_passed}} / ${{summary.assertions_total}}`],
  ['Workers / parallelism', `${{summary.workers}} / ${{summary.parallelism}}`],
  ['Step dwell', `${{summary.step_dwell_ms}} ms`],
];
document.getElementById('kpis').innerHTML = kpis.map(([label,value],i)=>
  `<div class="card"><span class="muted">${{esc(label)}}</span><span class="value ${{i===1?'good':i===2&&Number(value)?'bad':''}}">${{esc(value)}}</span></div>`).join('');

const targetRows = Object.entries(summary.targets).map(([name,t]) => [
  `<strong>${{esc(t.label || name)}}</strong>`, `${{t.passed}} / ${{t.episodes}}`,
  pct(t.passed,t.episodes), `<span class="${{t.failures?'bad':'good'}}">${{t.failures}}</span>`,
  ms(median(t.active_elapsed_ms)), num(t.resources?.average_cpu_percent),
  mib(t.resources?.peak_rss_bytes), mib(t.resources?.peak_pss_bytes),
  esc(JSON.stringify(t.status_counts || {{}})),
]);
document.getElementById('targets').innerHTML = table(
  ['Target','Passed','Pass rate','Failures','Median active','Avg CPU %','Peak RSS','Peak PSS','Statuses'], targetRows);

const operationRows = [];
for (const [name,t] of Object.entries(summary.targets)) for (const [operation,stat] of Object.entries(t.operations || {{}}))
  operationRows.push([esc(t.label || name),esc(operation),esc(stat.count ?? 0),ms(stat.p50),ms(stat.p90),ms(stat.max)]);
document.getElementById('operations').innerHTML = table(['Target','Operation','Correct n','p50','p90','max'],operationRows);

const caseRows = [];
for (const [name,t] of Object.entries(summary.targets)) for (const [caseName,c] of Object.entries(t.cases || {{}}))
  caseRows.push([esc(t.label || name),esc(caseName),`<span class="${{c.failures?'bad':'good'}}">${{c.failures}}</span>`,ms(median(c.elapsed_ms)),ms(median(c.active_elapsed_ms))]);
document.getElementById('cases').innerHTML = table(['Target','Episode','Failures','Median wall','Median active'],caseRows);

function linePath(samples,key,x0,y0,width,height,maxValue) {{
  const usable = samples.filter(s => s[key] != null && Number.isFinite(Number(s[key])));
  if (!usable.length || !maxValue) return '';
  const maxTime = Math.max(...samples.map(s=>Number(s.elapsed_ms)||0),1);
  return usable.map((s,i) => `${{i?'L':'M'}}${{x0 + width*(Number(s.elapsed_ms)||0)/maxTime}},${{y0 + height-height*Number(s[key])/maxValue}}`).join(' ');
}}
function timeline(target,payload) {{
  const samples = payload.samples || []; const width=680,height=155,x0=48,y0=18;
  if (!samples.length) return `<section class="panel chart"><h3>${{esc(target)}}</h3><div class="empty">No complete resource samples.</div></section>`;
  const memoryMax = Math.max(...samples.flatMap(s=>[Number(s.rss_bytes)||0,Number(s.pss_bytes)||0]),1);
  const cpuMax = Math.max(...samples.map(s=>Number(s.cpu_percent)||0),100);
  const memory = linePath(samples,'rss_bytes',x0,y0,width,height,memoryMax);
  const pss = linePath(samples,'pss_bytes',x0,y0,width,height,memoryMax);
  const cpu = linePath(samples,'cpu_percent',x0,y0,width,height,cpuMax);
  const start = Number(samples[0].timestamp); const span = Math.max(Number(samples.at(-1).timestamp)-start,.001);
  const markerLines = (data.phase_markers||[]).filter(m=>m.target===target && ['episode-start','episode-done','episode-failed'].includes(m.event))
    .map(m=>`<line class="marker" x1="${{x0+width*Math.max(0,Math.min(1,(Number(m.timestamp)-start)/span))}}" y1="${{y0}}" x2="${{x0+width*Math.max(0,Math.min(1,(Number(m.timestamp)-start)/span))}}" y2="${{y0+height}}"/>`).join('');
  return `<section class="panel chart"><h3>${{esc(summary.targets[target]?.label || target)}}</h3>
    <div class="legend"><span><i class="swatch rss"></i>RSS</span><span><i class="swatch pss"></i>PSS</span><span><i class="swatch cpu"></i>CPU (independent scale)</span></div>
    <svg viewBox="0 0 760 205" role="img" aria-label="resource timeline">
      <line class="axis" x1="${{x0}}" y1="${{y0+height}}" x2="${{x0+width}}" y2="${{y0+height}}"/>
      <line class="gridline" x1="${{x0}}" y1="${{y0+height/2}}" x2="${{x0+width}}" y2="${{y0+height/2}}"/>
      ${{markerLines}}<path class="series rss" d="${{memory}}"/><path class="series pss" d="${{pss}}"/><path class="series cpu" d="${{cpu}}"/>
      <text x="4" y="${{y0+5}}" fill="#91a3b5" font-size="11">${{esc(mib(memoryMax))}}</text>
      <text x="4" y="${{y0+height}}" fill="#91a3b5" font-size="11">0</text>
      <text x="${{x0+width-68}}" y="${{y0+height+18}}" fill="#91a3b5" font-size="11">${{num(span,1)}} s</text>
    </svg></section>`;
}}
document.getElementById('charts').innerHTML = Object.entries(data.resources || {{}}).map(([target,p])=>timeline(target,p)).join('');

const failures = data.episodes.filter(row=>!row.ok).map(row=>[
  esc(row.target),esc(row.episode),esc(row.run),esc(row.status),esc(row.failure_step),
  `<div class="error">${{esc(row.error)}}</div>`,row.failure_artifact?`<a href="${{esc(row.failure_artifact)}}">artifact</a>`:'—'
]);
document.getElementById('failures').innerHTML = table(['Target','Episode','Run','Status','Step','Error','Details'],failures);

const episodeRows = data.episodes.map(row=>[
  esc(row.target),esc(row.worker),esc(row.run),esc(row.episode),
  `<span class="${{row.ok?'good':'bad'}}">${{esc(row.status)}}</span>`,
  ms(row.elapsed_ms),ms(row.active_elapsed_ms),esc(row.step_count),esc(row.final_url)
]);
document.getElementById('episodes').innerHTML = table(['Target','Worker','Run','Episode','Status','Wall','Active','Steps','Final URL'],episodeRows);
</script>
</main></body></html>
"""
