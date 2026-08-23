"""Real-time live dashboard: HTML/JS served by the daemon + SSE event stream.

The daemon serves ``HTML_PAGE`` at ``/`` and streams engine events at
``/events`` (SSE). The page is a self-contained, dependency-free force-directed
graph renderer with full interactivity:

  - Click a node -> inspect panel pulls context / impact / neighbors from the
    existing ``/api/*`` REST endpoints (no new backend needed).
  - Search box -> highlights matching nodes.
  - Legend toggles -> filter edges by dimension.
  - Drag to pin a node; click again to release.
  - On every ``pass`` / ``cycle`` / ``reset_dim`` event the simulation reheats
    and node radii are re-ranked by references in-degree, so the graph visibly
    re-aligns in real time as the worm converges and as fs events trigger
    re-crawls.

``TerminalReporter`` is the ``--live`` terminal stream consumer.
"""

from __future__ import annotations

HTML_PAGE = '''<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>DataWorm — Live Network</title>
<style>
  :root{--bg:#0b0f19;--fg:#e6edf3;--dim:#8899aa;--contains:#94a3b8;--ref:#60a5fa;--dup:#fb923c;--sim:#c084fc;}
  *{box-sizing:border-box;margin:0;padding:0}
  body{font-family:system-ui,-apple-system,sans-serif;background:var(--bg);color:var(--fg);overflow:hidden;height:100vh;display:flex;flex-direction:column}
  header{padding:10px 16px;border-bottom:1px solid #1a2332;display:flex;align-items:center;justify-content:space-between;flex-shrink:0;gap:16px;flex-wrap:wrap}
  h1{font-size:1.05rem;font-weight:600;letter-spacing:.02em;white-space:nowrap}
  .badge{display:inline-block;padding:3px 8px;border-radius:999px;font-size:.72rem;background:#1a2332;border:1px solid #233040;font-variant-numeric:tabular-nums}
  .badge.ok{color:#4ade80;border-color:#164e30}
  .badge.run{color:#facc15;border-color:#4d3500}
  #legend{display:flex;gap:12px;font-size:.76rem;align-items:center}
  #legend button{display:inline-flex;align-items:center;gap:5px;background:transparent;border:1px solid #233040;color:var(--fg);border-radius:999px;padding:3px 8px;cursor:pointer;font-size:.76rem;opacity:1;transition:opacity .15s}
  #legend button.off{opacity:.3}
  #legend i{width:9px;height:9px;border-radius:50%;display:inline-block}
  #stats{font-size:.74rem;color:var(--dim);display:flex;gap:12px;font-variant-numeric:tabular-nums}
  #stats b{color:var(--fg)}
  #search{background:#0f1623;border:1px solid #233040;color:var(--fg);border-radius:6px;padding:4px 8px;font-size:.78rem;width:160px}
  #search:focus{outline:none;border-color:#60a5fa}
  main{flex:1;position:relative;overflow:hidden}
  canvas{display:block;width:100%;height:100%;cursor:default}
  canvas.grab{cursor:grab}
  canvas.grabbing{cursor:grabbing}
  #overlay{position:absolute;top:0;left:0;pointer-events:none;padding:12px;line-height:1.45;font-size:.78rem;color:var(--fg);text-shadow:0 1px 4px rgba(0,0,0,.85);max-width:340px}
  #overlay .line{margin-bottom:2px}
  #overlay .dim{color:var(--dim)}
  /* Inspect panel */
  #panel{position:absolute;top:0;right:0;width:340px;height:100%;background:#0d1320;border-left:1px solid #1a2332;overflow-y:auto;padding:14px 16px;font-size:.8rem;transform:translateX(100%);transition:transform .2s;box-shadow:-4px 0 16px rgba(0,0,0,.4)}
  #panel.open{transform:translateX(0)}
  #panel h2{font-size:.86rem;font-weight:600;margin-bottom:4px;word-break:break-all}
  #panel .meta{color:var(--dim);font-size:.72rem;margin-bottom:10px}
  #panel h3{font-size:.74rem;text-transform:uppercase;letter-spacing:.04em;color:var(--dim);margin:12px 0 4px;border-bottom:1px solid #1a2332;padding-bottom:3px}
  #panel ul{list-style:none;margin:0 0 4px}
  #panel li{padding:2px 0;color:var(--fg);font-size:.76rem;word-break:break-all;cursor:pointer}
  #panel li:hover{color:#60a5fa}
  #panel .pill{display:inline-block;background:#162033;border:1px solid #233040;border-radius:999px;padding:1px 7px;margin:1px;font-size:.7rem}
  #panel .close{position:absolute;top:8px;right:10px;cursor:pointer;color:var(--dim);font-size:1.1rem;background:none;border:none}
  #panel .close:hover{color:var(--fg)}
  #tooltip{position:absolute;pointer-events:none;background:#0d1320;border:1px solid #233040;border-radius:5px;padding:4px 8px;font-size:.72rem;color:var(--fg);display:none;z-index:10;max-width:280px;word-break:break-all}
  /* Inspect: WHAT / WHERE sections */
  #panel .kv{display:flex;gap:6px;padding:2px 0;font-size:.74rem;align-items:baseline}
  #panel .kv .k{color:var(--dim);min-width:58px;flex-shrink:0}
  #panel .kv .v{color:var(--fg);word-break:break-all}
  #panel .what{background:#101a2b;border-left:3px solid #60a5fa;padding:6px 9px;border-radius:4px;margin-bottom:8px}
  #panel .where{background:#0e1c18;border-left:3px solid #34d399;padding:6px 9px;border-radius:4px;margin-bottom:8px}
  #panel .endpoint{background:#0f1623;border:1px solid #233040;border-radius:6px;padding:6px 8px;margin:4px 0;cursor:pointer}
  #panel .endpoint:hover{border-color:#60a5fa}
  #panel .endpoint .t{font-size:.74rem;color:var(--fg);word-break:break-all}
  #panel .endpoint .s{font-size:.68rem;color:var(--dim);word-break:break-all}
  #panel .btn{display:inline-block;margin:4px 6px 0 0;background:#162033;border:1px solid #233040;color:var(--fg);border-radius:6px;padding:4px 9px;cursor:pointer;font-size:.74rem}
  #panel .btn:hover{border-color:#60a5fa}
  #panel .btn.primary{border-color:#60a5fa;color:#60a5fa}
  #panel h3:first-of-type{margin-top:6px}
</style>
</head>
<body>
<header>
  <h1>DataWorm — Live Network</h1>
  <div id="legend">
    <button data-dim="contains"><i style="background:var(--contains)"></i>contains</button>
    <button data-dim="references"><i style="background:var(--ref)"></i>references</button>
    <button data-dim="duplicate_of"><i style="background:var(--dup)"></i>duplicate</button>
    <button data-dim="similar_to"><i style="background:var(--sim)"></i>similar</button>
  </div>
  <div id="stats">
    <span>nodes: <b id="n_nodes">0</b></span>
    <span>cont: <b id="n_cont">0</b></span>
    <span>ref: <b id="n_ref">0</b></span>
    <span>dup: <b id="n_dup">0</b></span>
    <span>sim: <b id="n_sim">0</b></span>
    <span>frags: <b id="n_frags">0</b></span>
    <input id="search" type="text" placeholder="search paths..." />
  </div>
  <span id="status" class="badge run">live</span>
</header>
<main>
  <canvas id="net"></canvas>
  <div id="overlay">
    <div class="line">Cycle: <b id="v_cycle">—</b></div>
    <div class="line">Pass: <b id="v_pass">—</b></div>
    <div class="line">Status: <b id="v_stat">running</b></div>
    <div class="line dim" style="margin-top:6px">click node/block to expand · click link to inspect · drag to pin · scroll to zoom · legend toggles layers</div>
  </div>
  <div id="panel">
    <button class="close" onclick="closePanel()">×</button>
    <h2 id="p_title">—</h2>
    <div class="meta" id="p_meta"></div>
    <div id="p_body"></div>
  </div>
  <div id="tooltip"></div>
</main>
<script>
// ── DataWorm scale-free dashboard ──────────────────────────────────────────
// Design: the graph can hold millions of nodes without freezing because we
// never simulate/draw all of them. A viewport transform (pan/zoom) plus a
// quadtree mean only the ~hundreds of nodes in the visible window are
// physics-simulated and drawn each frame. At low zoom, directories collapse
// into cluster nodes so the "whole tree" view shows structure (hundreds of
// dirs), not 1M unreadable dots. Counters are maintained incrementally (no
// per-event full scan). Node ingest is batched (nodes_batch), not per-node.

const canvas = document.getElementById('net');
const ctx = canvas.getContext('2d');
// The daemon injects the real bearer token here at serve time (see server.py).
// Every /api fetch + the /events SSE stream send it; ops are deny-by-default.
const TOKEN = "__DATAWORM_TOKEN__";
const AUTH_HEADERS = {'Authorization': 'Bearer ' + TOKEN};
let W = canvas.width = window.innerWidth, H = canvas.height = window.innerHeight;
window.addEventListener('resize', () => { W = canvas.width = window.innerWidth; H = canvas.height = window.innerHeight; });

// ── State ───────────────────────────────────────────────────────────────────
const nodes = {};            // id -> {x,y,vx,vy,kind,path,fixed,score,childCount?}
const edges = {};            // "src|dst|type" -> {src,dst,type,weight}
const dimOn = {contains:true, references:true, duplicate_of:true, similar_to:true};
// Running counters — O(1) per event, NOT a full scan (was countEdges, O((N+E)^2)).
const cnt = {nodes:0, contains:0, references:0, duplicate_of:0, similar_to:0};
let hoveredId = null, selectedId = null, selectedEdge = null, dragNode = null, dragNodeId = null, dragOffsetX = 0, dragOffsetY = 0;
let searchHits = new Set();
let dropped = 0;
// ── Expand / collapse "blocks" (collapsed directory subtrees) ───────────────
// `expanded`  : dir ids the user has opened (drawn as normal nodes + halo).
// `hidden`    : node ids buried inside a collapsed block (not drawn/simulated).
// `childrenOf`/`childCount`/`parentOf` : structure from `contains` edges, so a
//   dir with many children collapses into a single clickable "block".
const expanded = new Set();
const hidden = new Set();
const childrenOf = {};      // dir id -> [child ids]
const childCount = {};      // dir id -> number of contains children
const parentOf = {};        // child id -> parent dir id
let viewTarget = null;      // {scale,ox,oy} for smooth focus/zoom animation
let lastClusterScale = null;// last scale where we recomputed block clustering
// Mouse helpers for distinguishing a click from a pan-drag.
let panMoved = false, downWX = 0, downWY = 0;

const COL = {contains:'#94a3b8', references:'#60a5fa', duplicate_of:'#fb923c', similar_to:'#c084fc'};
const KIND_COLOR = {dir:'#f59e0b', file:'#38bdf8'};
const REST = {contains:70, references:120, duplicate_of:90, similar_to:110};

// ── Viewport transform (pan + zoom) ─────────────────────────────────────────
// world = (screen - offset) / scale. We render in screen space by applying
// ctx.translate/scale, but physics + hit-testing work in world space.
let view = {ox: 0, oy: 0, scale: 1};   // offset in screen coords, scale factor
function screenToWorld(sx, sy) { return [(sx - view.ox) / view.scale, (sy - view.oy) / view.scale]; }
function worldToScreen(wx, wy) { return [wx * view.scale + view.ox, wy * view.scale + view.oy]; }
// Visible world bounds for culling.
function visibleBounds() {
  const [x0, y0] = screenToWorld(0, 0);
  const [x1, y1] = screenToWorld(W, H);
  return {x0, y0, x1, y1};
}

// Center the viewport on the graph's centroid so the graph appears in the
// middle of the screen, not the top-left corner.
function centerView() {
  const ids = Object.keys(nodes);
  if (!ids.length) return;
  let cx = 0, cy = 0;
  for (const id of ids) { cx += nodes[id].x; cy += nodes[id].y; }
  cx /= ids.length; cy /= ids.length;
  // Map the centroid to the screen center.
  view.ox = W/2 - cx * view.scale;
  view.oy = H/2 - cy * view.scale;
  QT.dirty = true;
}

// ── Quadtree for O(log n) hit-testing + viewport queries ─────────────────────
// A minimal quadtree over node world positions. Rebuilt lazily when dirty.
const QT = {dirty: true, root: null, MAX: 8, DEPTH: 12};
function qtNode(x0,y0,x1,y1) { return {x0,y0,x1,y1, pts:[], kids:null}; }
function qtInsert(n, p, depth) {
  if (depth >= QT.DEPTH) { n.pts.push(p); return; }
  if (n.pts.length < QT.MAX && !n.kids) { n.pts.push(p); return; }
  if (!n.kids) {
    // split
    const mx=(n.x0+n.x1)/2, my=(n.y0+n.y1)/2;
    n.kids = [qtNode(n.x0,my,mx,n.y1), qtNode(mx,my,n.x1,n.y1), qtNode(n.x0,n.y0,mx,my), qtNode(mx,n.y0,n.x1,my)];
    // re-insert existing pts
    const old = n.pts; n.pts = [];
    for (const op of old) qtInsert(n, op, depth);
  }
  const px = p.x, py = p.y;
  const mx=(n.x0+n.x1)/2, my=(n.y0+n.y1)/2;
  let i = (px>=mx?1:0)+(py>=my?2:0); // child index: 0=NW... adjust below
  // kids order: [SW, SE, NW, NE] by (x>=mx, y>=my) — fix mapping
  if (px < mx && py < my) i = 0;
  else if (px >= mx && py < my) i = 1;
  else if (px < mx && py >= my) i = 2;
  else i = 3;
  qtInsert(n.kids[i], p, depth+1);
}
function qtQuery(n, bounds, out) {
  if (!n) return;
  if (n.x1 < bounds.x0 || n.x0 > bounds.x1 || n.y1 < bounds.y0 || n.y0 > bounds.y1) return; // outside
  for (const p of n.pts) if (p.x>=bounds.x0 && p.x<=bounds.x1 && p.y>=bounds.y0 && p.y<=bounds.y1) out.push(p);
  if (n.kids) for (const k of n.kids) qtQuery(k, bounds, out);
}
function qtFindAt(n, x, y, best) {
  if (!n) return;
  if (x < n.x0 || x > n.x1 || y < n.y0 || y > n.y1) return;
  for (const p of n.pts) {
    if (hidden.has(p.id)) continue;
    const dx = x-p.x, dy = y-p.y;
    const r = (p.node.kind==='dir'?7:5)/view.scale + 3/view.scale;
    const d = dx*dx+dy*dy;
    if (d < r*r && d < best.d) { best.d = d; best.p = p; }
  }
  if (n.kids) for (const k of n.kids) qtFindAt(k, x, y, best);
}
function rebuildQuadtree() {
  // Compute bounds of all node positions.
  let x0=Infinity,y0=Infinity,x1=-Infinity,y1=-Infinity;
  for (const id in nodes) { const n=nodes[id]; if(n.x<x0)x0=n.x; if(n.y<y0)y0=n.y; if(n.x>x1)x1=n.x; if(n.y>y1)y1=n.y; }
  if (x0===Infinity) { QT.root=null; return; }
  QT.root = qtNode(x0,y0,x1+1,y1+1);
  for (const id in nodes) { const n=nodes[id]; qtInsert(QT.root, {x:n.x, y:n.y, id, node:n}, 0); }
  QT.dirty = false;
}

// ── Clustering: at low zoom, directories collapse into cluster nodes ─────────
// A node is "clustered" (rendered as a single dot representing its subtree)
// when its displayed radius would be < ~3px. We don't merge data; we just skip
// drawing the children of clustered dirs and draw the dir bigger instead.
function clusterThreshold() { return 8 / view.scale; } // dir visible if > 8 world units
// A dir becomes a collapsed "block" only when it is big (many children) AND the
// user has zoomed out far enough — and only if the user hasn't expanded it.
function isClusteredDir(id) {
  const n = nodes[id];
  if (!n || n.kind !== 'dir') return false;
  if (expanded.has(id)) return false;
  if ((childCount[id] || 0) <= 3) return false;
  return view.scale < 0.5;
}
function isHidden(id) { return hidden.has(id); }

// Recompute which nodes are buried inside collapsed blocks. O(edges) via the
// `childrenOf` adjacency (BFS from every collapsed dir).
function recomputeHidden() {
  hidden.clear();
  for (const id in nodes) {
    if (!isClusteredDir(id)) continue;
    const st = (childrenOf[id] || []).slice();
    while (st.length) {
      const c = st.pop();
      if (hidden.has(c)) continue;
      hidden.add(c);
      const kids = childrenOf[c];
      if (kids) for (const g of kids) st.push(g);
    }
  }
  QT.dirty = true;
}

// Rebuild the family maps from scratch (after a `contains` reset or at `done`).
function rebuildFamilyMaps() {
  for (const k in childCount) delete childCount[k];
  for (const k in childrenOf) delete childrenOf[k];
  for (const k in parentOf) delete parentOf[k];
  for (const key in edges) {
    const e = edges[key];
    if (e.type !== 'contains') continue;
    childCount[e.src] = (childCount[e.src] || 0) + 1;
    (childrenOf[e.src] = childrenOf[e.src] || []).push(e.dst);
    parentOf[e.dst] = e.src;
  }
  recomputeHidden();
}

// Smoothly animate the viewport to center `id` at `targetScale`.
function focusOn(id, targetScale) {
  const n = nodes[id]; if (!n) return;
  targetScale = Math.max(0.05, Math.min(6, targetScale || 1.4));
  viewTarget = { scale: targetScale, ox: W/2 - n.x*targetScale, oy: H/2 - n.y*targetScale };
}

// Closest distance from point (px,py) to segment (ax,ay)-(bx,by).
function distToSeg(px, py, ax, ay, bx, by) {
  const dx = bx-ax, dy = by-ay; const l2 = dx*dx + dy*dy;
  let t = l2 ? ((px-ax)*dx + (py-ay)*dy) / l2 : 0;
  t = Math.max(0, Math.min(1, t));
  const cx = ax + t*dx, cy = ay + t*dy;
  return Math.hypot(px - cx, py - cy);
}

// Hit-test a visible edge near world point (wx,wy); returns {src,dst,type,weight}.
function edgeAt(wx, wy) {
  if (Object.keys(edges).length > 60000) return null; // too dense to pick
  const b = visibleBounds(); const m = 20/view.scale; b.x0 -= m; b.y0 -= m; b.x1 += m; b.y1 += m;
  let best = null, bestD = 6/view.scale; // tolerance in world units
  for (const k in edges) {
    const e = edges[k]; if (!dimOn[e.type]) continue;
    const u = nodes[e.src], v = nodes[e.dst]; if (!u || !v) continue;
    if (hidden.has(e.src) || hidden.has(e.dst)) continue;
    if ((u.x<b.x0&&v.x<b.x0)||(u.x>b.x1&&v.x>b.x1)||(u.y<b.y0&&v.y>b.y1)||(u.y>b.y1&&v.y>b.y0)) continue;
    const d = distToSeg(wx, wy, u.x, u.y, v.x, v.y);
    if (d < bestD) { bestD = d; best = {src:e.src, dst:e.dst, type:e.type, weight:e.weight}; }
  }
  return best;
}

function roundRect(c, x, y, w, h, r) {
  c.beginPath();
  c.moveTo(x+r, y);
  c.arcTo(x+w, y, x+w, y+h, r);
  c.arcTo(x+w, y+h, x, y+h, r);
  c.arcTo(x, y+h, x, y, r);
  c.arcTo(x, y, x+w, y, r);
  c.closePath();
}

function esc(s) { return String(s==null?'':s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
function js(s) { return '"' + String(s).replace(/\\/g, '\\\\').replace(/"/g, '\\"') + '"'; }
function short(p) { if (!p) return ''; const parts = String(p).split(/[\\/]/); return parts[parts.length-1] || p; }

// ── Physics: only on the visible window (+ margin) ──────────────────────────
let reheat = 0;
function physics() {
  const b = visibleBounds();
  const m = 80 / view.scale; // margin in world units
  b.x0 -= m; b.y0 -= m; b.x1 += m; b.y1 += m;
  // Collect visible node refs (not a full scan of `nodes` — a quadtree query).
  const vis = []; if (QT.root) qtQuery(QT.root, b, vis);
  // O(v^2) repulsion where v = visible count (hundreds at sane zoom).
  for (let i=0;i<vis.length;i++) {
    const a = vis[i].node;
    if (a.fixed || hidden.has(vis[i].id)) continue;
    for (let j=i+1;j<vis.length;j++) {
      const bb = vis[j].node;
      if (bb.fixed) continue;
      const dx=a.x-bb.x, dy=a.y-bb.y;
      let dist=Math.sqrt(dx*dx+dy*dy)||1; const minD=55;
      if(dist<minD)dist=minD;
      const f=1100/(dist*dist), fx=(dx/dist)*f, fy=(dy/dist)*f;
      a.vx+=fx; a.vy+=fy; bb.vx-=fx; bb.vy-=fy;
    }
    a.vx += (0-a.x)*0.0004; a.vy += (0-a.y)*0.0004; // gentle gravity to world origin
    if (reheat>0){ a.vx+=(Math.random()-0.5)*reheat*2; a.vy+=(Math.random()-0.5)*reheat*2; }
    a.vx*=0.90; a.vy*=0.90;
  }
  if (reheat>0) reheat=Math.max(0,reheat-0.4);
  // Spring edges — only those with both endpoints visible (or near-visible).
  // For speed at scale we skip edges entirely above ~50k visible nodes.
  if (vis.length < 2000) {
    for (const k in edges) {
      const e = edges[k];
      if (!dimOn[e.type]) continue;
      const u=nodes[e.src], v=nodes[e.dst];
      if (!u||!v) continue;
      if (hidden.has(e.src)||hidden.has(e.dst)) continue;
      // cheap viewport reject: if both endpoints far outside bounds, skip
      if ((u.x<b.x0&&v.x<b.x0)||(u.x>b.x1&&v.x>b.x1)||(u.y<b.y0&&v.y<b.y0)||(u.y>b.y1&&v.y>b.y1)) continue;
      const dx=v.x-u.x, dy=v.y-u.y, dist=Math.sqrt(dx*dx+dy*dy)||1;
      const rest=REST[e.type]||90, f=(dist-rest)*0.013, fx=(dx/dist)*f, fy=(dy/dist)*f;
      if(!u.fixed){u.vx+=fx;u.vy+=fy;} if(!v.fixed){v.vx-=fx;v.vy-=fy;}
    }
  }
}

// ── Draw: only visible nodes/edges, with viewport transform ─────────────────
function draw() {
  ctx.fillStyle='#0b0f19'; ctx.fillRect(0,0,W,H);
  ctx.save();
  ctx.translate(view.ox, view.oy); ctx.scale(view.scale, view.scale);
  const b = visibleBounds(); const m = 20/view.scale; b.x0-=m; b.y0-=m; b.x1+=m; b.y1+=m;
  // Edges (batched stroke per type for fewer canvas calls).
  if (QT.root && countVisible() < 2000) {
    const byType = {contains:[],references:[],duplicate_of:[],similar_to:[]};
    for (const k in edges) { const e=edges[k]; if(dimOn[e.type]) byType[e.type].push(e); }
    for (const t in byType) {
      const arr = byType[t];
      if (!arr.length) continue;
      ctx.strokeStyle = COL[t]||'#8899aa'; ctx.lineWidth = 1.0/view.scale; ctx.globalAlpha = 0.45;
      ctx.beginPath();
      for (const e of arr) {
        const u=nodes[e.src], v=nodes[e.dst]; if(!u||!v) continue;
        if (hidden.has(e.src)||hidden.has(e.dst)) continue;
        if ((u.x<b.x0&&v.x<b.x0)||(u.x>b.x1&&v.x>b.x1)||(u.y<b.y0&&v.y>b.y0)||(u.y>b.y1&&v.y>b.y1)) continue;
        ctx.moveTo(u.x,u.y); ctx.lineTo(v.x,v.y);
      }
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
    // Highlight the currently selected link on top of everything.
    if (selectedEdge) {
      const u=nodes[selectedEdge.src], v=nodes[selectedEdge.dst];
      if (u&&v) {
        ctx.strokeStyle='#facc15'; ctx.lineWidth=2.6/view.scale; ctx.globalAlpha=0.95;
        ctx.beginPath(); ctx.moveTo(u.x,u.y); ctx.lineTo(v.x,v.y); ctx.stroke();
        ctx.globalAlpha=1;
      }
    }
  }
  // Nodes — query the quadtree for the visible window only.
  const vis = []; if (QT.root) qtQuery(QT.root, b, vis);
  for (const p of vis) {
    const id = p.id;
    if (hidden.has(id)) continue;
    const n = p.node;
    const isExp = expanded.has(id);
    const clustered = isClusteredDir(id);
    const baseR = n.kind==='dir'?6:3.2;
    const r = clustered ? Math.min(6 + Math.sqrt(childCount[id]||0)*2, 18) : (n.score?Math.min(baseR+n.score*1.6,12):baseR);
    const rr = (id===hoveredId||id===selectedId) ? r+2/view.scale : r;
    const isHit = searchHits.size && searchHits.has(id);
    if (clustered) {
      // A collapsed directory is drawn as a single "block" you can click to expand.
      const w = rr*2, h = rr*2;
      ctx.fillStyle = isHit ? '#facc15' : '#f59e0b';
      roundRect(ctx, n.x - w/2, n.y - h/2, w, h, Math.max(3, rr*0.35));
      ctx.fill();
      ctx.strokeStyle = id===selectedId ? '#facc15' : '#0b0f19';
      ctx.lineWidth = (id===selectedId ? 2.5 : 1.3)/view.scale;
      ctx.stroke();
      const cc = childCount[id]||0;
      if (cc) {
        ctx.font = (10/view.scale)+'px system-ui'; ctx.fillStyle='#0b0f19';
        ctx.textAlign='center'; ctx.textBaseline='middle';
        ctx.fillText(String(cc), n.x, n.y);
        ctx.textAlign='start'; ctx.textBaseline='alphabetic';
      }
    } else {
      ctx.beginPath(); ctx.arc(n.x,n.y,rr,0,Math.PI*2);
      ctx.fillStyle = isHit ? '#facc15' : (KIND_COLOR[n.kind]||'#fff');
      ctx.fill();
      ctx.strokeStyle = id===selectedId ? '#facc15' : (isExp ? '#34d399' : '#0b0f19');
      ctx.lineWidth = (id===selectedId ? 2.5 : (isExp ? 2 : 1.3))/view.scale;
      ctx.stroke();
    }
    if (isExp) { // halo marks an expanded (open) block
      ctx.beginPath(); ctx.arc(n.x, n.y, rr+4/view.scale, 0, Math.PI*2);
      ctx.strokeStyle='rgba(52,211,153,0.75)'; ctx.lineWidth=1.5/view.scale; ctx.stroke();
    }
  }
  ctx.restore();
  // Hover label (screen space, on top).
  if (hoveredId && nodes[hoveredId]) {
    const n = nodes[hoveredId];
    const [sx,sy] = worldToScreen(n.x, n.y);
    const label = n.path || hoveredId;
    ctx.font = '11px system-ui';
    const tw = ctx.measureText(label).width + 8;
    ctx.fillStyle = 'rgba(13,19,32,0.92)'; ctx.fillRect(sx+8, sy-18, tw, 16);
    ctx.fillStyle = '#e6edf3'; ctx.fillText(label, sx+12, sy-6);
  }
}

function countVisible() {
  const b = visibleBounds(); let c=0;
  if (QT.root) { const a=[]; qtQuery(QT.root,b,a); c=a.length; }
  return c;
}

function tick() {
  // Smoothly animate toward a focus target (used by Expand / Zoom-to / click).
  if (viewTarget) {
    view.scale += (viewTarget.scale - view.scale) * 0.18;
    view.ox += (viewTarget.ox - view.ox) * 0.18;
    view.oy += (viewTarget.oy - view.oy) * 0.18;
    QT.dirty = true;
    if (Math.abs(view.scale-viewTarget.scale) < 0.002 &&
        Math.abs(view.ox-viewTarget.ox) < 0.5 && Math.abs(view.oy-viewTarget.oy) < 0.5) {
      view.scale = viewTarget.scale; view.ox = viewTarget.ox; view.oy = viewTarget.oy; viewTarget = null;
    }
  }
  // When the zoom crosses the collapse threshold, re-evaluate which dirs are blocks.
  if (lastClusterScale === null || (lastClusterScale >= 0.5) !== (view.scale >= 0.5)) {
    recomputeHidden();
    lastClusterScale = view.scale;
  }
  if (QT.dirty && Object.keys(nodes).length < 200000) rebuildQuadtree();
  physics();
  // Integrate positions (only visible nodes move meaningfully; cheap enough
  // to loop `nodes` at up to ~100k; above that we only integrate visible ones).
  const big = Object.keys(nodes).length > 100000;
  if (!big) {
    for (const id in nodes) { const n=nodes[id]; if(n.fixed||hidden.has(id))continue; n.x+=n.vx; n.y+=n.vy;
      if(n.x<-50000){n.x=-50000;n.vx*=-0.3;} if(n.x>50000){n.x=50000;n.vx*=-0.3;}
      if(n.y<-50000){n.y=-50000;n.vy*=-0.3;} if(n.y>50000){n.y=50000;n.vy*=-0.3;} }
  } else {
    const b=visibleBounds(); const m=200/view.scale; b.x0-=m;b.y0-=m;b.x1+=m;b.y1+=m;
    const vis=[]; if(QT.root) qtQuery(QT.root,b,vis);
    for (const p of vis) { const n=p.node; if(n.fixed||hidden.has(p.id))continue; n.x+=n.vx; n.y+=n.vy; }
  }
  if (dragNode) { dragNode.x = dragWX; dragNode.y = dragWY; }
  draw();
  requestAnimationFrame(tick);
}

// ── Running counters: O(1) per event (replaces countEdges full-scan) ──────────
function updateCounters() {
  document.getElementById('n_nodes').textContent = cnt.nodes;
  document.getElementById('n_cont').textContent = cnt.contains;
  document.getElementById('n_ref').textContent = cnt.references;
  document.getElementById('n_dup').textContent = cnt.duplicate_of;
  document.getElementById('n_sim').textContent = cnt.similar_to;
}

// ── Ranking: recompute references in-degree on cycle (capped scan) ──────────
function recomputeScores() {
  // Only rescore visible + their neighbors to stay cheap at scale; full rescore
  // is O(n+e) which is fine up to ~50k, beyond that we skip (radii just stay).
  if (Object.keys(nodes).length > 50000) return;
  for (const id in nodes) nodes[id].score = 0;
  for (const k in edges) if (edges[k].type==='references' && nodes[edges[k].dst]) nodes[edges[k].dst].score += 1;
}

// ── Mouse: hit-test via quadtree, pan, zoom, drag/pin ────────────────────────
let panStart = null, dragWX = 0, dragWY = 0;
canvas.addEventListener('mousedown', (e) => {
  const rect = canvas.getBoundingClientRect();
  const sx = e.clientX-rect.left, sy = e.clientY-rect.top;
  const [wx, wy] = screenToWorld(sx, sy);
  // Hit-test via quadtree (O(log n), not O(n)).
  const best = {d: Infinity, p: null};
  if (QT.root) qtFindAt(QT.root, wx, wy, best);
  if (best.p) {
    dragNode = best.p.node; dragNodeId = best.p.id; dragWX = best.p.x; dragWY = best.p.y;
    dragOffsetX = wx - best.p.x; dragOffsetY = wy - best.p.y;
    dragNode.fixed = true;
    canvas.className = 'grabbing';
  } else {
    // Pan: start dragging the background (or, on click, pick a link).
    panStart = {sx, sy, ox: view.ox, oy: view.oy};
    panMoved = false; downWX = wx; downWY = wy;
    canvas.className = 'grabbing';
  }
});
canvas.addEventListener('mousemove', (e) => {
  const rect = canvas.getBoundingClientRect();
  const sx = e.clientX-rect.left, sy = e.clientY-rect.top;
  if (panStart) {
    if (Math.hypot(sx-panStart.sx, sy-panStart.sy) > 4) panMoved = true;
    view.ox = panStart.ox + (sx - panStart.sx);
    view.oy = panStart.oy + (sy - panStart.sy);
    return;
  }
  if (dragNode) {
    const [wx, wy] = screenToWorld(sx, sy);
    dragWX = wx - dragOffsetX; dragWY = wy - dragOffsetY;
    return;
  }
  const [wx, wy] = screenToWorld(sx, sy);
  const best = {d: Infinity, p: null};
  if (QT.root) qtFindAt(QT.root, wx, wy, best);
  const id = best.p ? best.p.id : null;
  const tt = document.getElementById('tooltip');
  if (id !== hoveredId) { hoveredId = id; canvas.className = id ? 'grab' : ''; }
  if (id) {
    tt.style.display='block'; tt.style.left=(sx+14)+'px'; tt.style.top=(sy+14)+'px';
    tt.textContent = short(nodes[id].path) || id;
  } else if (Object.keys(edges).length <= 5000) {
    // Hovering empty space: surface a nearby link so it's discoverable.
    const ev = edgeAt(wx, wy);
    if (ev) {
      tt.style.display='block'; tt.style.left=(sx+14)+'px'; tt.style.top=(sy+14)+'px';
      tt.textContent = ev.type + ': ' + short(nodes[ev.src] && nodes[ev.src].path) + ' → ' + short(nodes[ev.dst] && nodes[ev.dst].path);
    } else { tt.style.display='none'; }
  } else { tt.style.display='none'; }
});
canvas.addEventListener('mouseup', (e) => {
  const rect = canvas.getBoundingClientRect();
  const sx = e.clientX-rect.left, sy = e.clientY-rect.top;
  if (panStart) {
    if (!panMoved) {
      // A plain click on empty space selects the nearest link, else clears.
      const best = edgeAt(downWX, downWY);
      if (best) selectEdge(best); else clearSelection();
    }
    panStart = null; canvas.className = hoveredId?'grab':''; return;
  }
  if (dragNode) {
    const moved = Math.hypot(dragNode.x - dragWX, dragNode.y - dragWY) > 4/view.scale;
    if (!moved && dragNodeId) {
      const n = nodes[dragNodeId];
      // Clicking a collapsed block expands it (reveals + zooms to its children).
      if (n && n.kind==='dir' && isClusteredDir(dragNodeId)) {
        expanded.add(dragNodeId); recomputeHidden(); focusOn(dragNodeId, 1.5);
      }
      inspectNode(dragNodeId);
    }
    dragNode = null; dragNodeId = null; canvas.className = hoveredId?'grab':'';
  }
});
canvas.addEventListener('mouseleave', () => {
  hoveredId=null; panStart=null; dragNode=null;
  document.getElementById('tooltip').style.display='none';
  canvas.className='';
});
// Zoom: wheel scales around the cursor.
canvas.addEventListener('wheel', (e) => {
  e.preventDefault();
  const rect = canvas.getBoundingClientRect();
  const sx = e.clientX-rect.left, sy = e.clientY-rect.top;
  const [wx, wy] = screenToWorld(sx, sy);
  const factor = e.deltaY < 0 ? 1.2 : 1/1.2;
  view.scale = Math.max(0.02, Math.min(8, view.scale * factor));
  // Keep the world point under the cursor stationary.
  view.ox = sx - wx*view.scale; view.oy = sy - wy*view.scale;
  QT.dirty = true;
}, {passive:false});

// ── Dimension filter toggles ─────────────────────────────────────────────────
document.getElementById('legend').addEventListener('click', (e) => {
  const btn = e.target.closest('button'); if (!btn) return;
  const dim = btn.dataset.dim; dimOn[dim] = !dimOn[dim];
  btn.classList.toggle('off', !dimOn[dim]);
});

// ── Search (capped at 200 by the backend) ────────────────────────────────────
let searchTimer = null;
document.getElementById('search').addEventListener('input', (e) => {
  const q = e.target.value.trim();
  clearTimeout(searchTimer);
  if (!q) { searchHits.clear(); return; }
  searchTimer = setTimeout(async () => {
    try {
      const r = await fetch('/api/search?text='+encodeURIComponent(q)+'&limit=200', {headers: AUTH_HEADERS});
      const data = await r.json();
      searchHits = new Set((data.results||[]).map(x=>x.id||x.path||x));
    } catch(_) { searchHits.clear(); }
  }, 200); // debounce: don't fire per keystroke
});

// ── Inspect panel: capped lists (no unbounded DOM) ──────────────────────────
const INSPECT_CAP = 100;
function closePanel() { document.getElementById('panel').classList.remove('open'); selectedId=null; selectedEdge=null; }

// Clear any selection and hide the panel.
function clearSelection() { selectedEdge=null; selectedId=null; document.getElementById('panel').classList.remove('open'); }

// Center the viewport on a node (used by "Center" button).
function locate(id) { selectedId=id; focusOn(id, Math.max(view.scale, 1.4)); }

// Expand/collapse a directory block.
function toggleExpand(id) {
  if (expanded.has(id)) { expanded.delete(id); recomputeHidden(); }
  else { expanded.add(id); recomputeHidden(); focusOn(id, 1.5); }
  inspectNode(id);
}

// Inspect a link (edge): what it means + where each endpoint lives.
function selectEdge(e) { selectedEdge=e; selectedId=null; inspectEdge(e); }
function endpointHTML(id, n) {
  const name = n.path ? short(n.path) : id;
  return '<div class="endpoint" onclick="inspectNode('+js(id)+')">'
    + '<div class="t">'+esc(name)+'</div>'
    + '<div class="s">'+(n.kind||'?')+(n.path?' · '+esc(n.path):'')+'</div>'
    + '</div>';
}
const REL_MEANING = {
  contains: 'structural — a parent directory contains this node',
  references: 'content — a file imports / links to the target',
  duplicate_of: 'identical or near-identical content (sha256 / simhash)',
  similar_to: 'semantic similarity above the threshold (embedding cosine)'
};
function inspectEdge(e) {
  selectedEdge = e; selectedId = null;
  const panel = document.getElementById('panel');
  document.getElementById('p_title').textContent = e.type;
  document.getElementById('p_meta').textContent = 'link · ' + e.src + ' → ' + e.dst;
  const a = nodes[e.src] || {}, c = nodes[e.dst] || {};
  let html = '';
  html += '<div class="what">';
  html += '<div class="kv"><span class="k">what</span><span class="v">'+esc(e.type)+'</span></div>';
  html += '<div class="kv"><span class="k">means</span><span class="v">'+esc(REL_MEANING[e.type]||'relationship')+'</span></div>';
  html += '<div class="kv"><span class="k">weight</span><span class="v">'+(e.weight!=null?e.weight:'1')+'</span></div>';
  html += '</div>';
  html += '<h3>where — source</h3>' + endpointHTML(e.src, a);
  html += '<h3>where — target</h3>' + endpointHTML(e.dst, c);
  html += '<div style="margin:8px 0">'
    + '<span class="btn" onclick="inspectNode('+js(e.src)+')">Open source</span>'
    + '<span class="btn" onclick="inspectNode('+js(e.dst)+')">Open target</span>'
    + '</div>';
  document.getElementById('p_body').innerHTML = html;
  panel.classList.add('open');
}

async function inspectNode(id) {
  selectedId = id; selectedEdge = null;
  const n = nodes[id] || {};
  const panel = document.getElementById('panel');
  document.getElementById('p_title').textContent = n.path ? short(n.path) : id;
  // Show the parent directory + kind clearly so you know which dir this is in.
  const parentDir = n.path ? n.path.replace(/[\\/][^\\/]+$/, '') : '';
  const parName = (parentDir.split(/[\\/]/).pop()) || '(root)';
  document.getElementById('p_meta').textContent = (n.kind || '?') + (parName ? ' · in ' + parName : '') + ' · ' + id;
  document.getElementById('p_body').innerHTML = '<div class="dim">loading…</div>';
  panel.classList.add('open');
  const enc = encodeURIComponent(id);
  let html = '';
  try {
    const [ctxR, impR, nbR] = await Promise.all([
      fetch('/api/context?path='+enc, {headers: AUTH_HEADERS}).then(r=>r.json()).catch(()=>({})),
      fetch('/api/impact?path='+enc, {headers: AUTH_HEADERS}).then(r=>r.json()).catch(()=>({})),
      fetch('/api/neighbors?path='+enc+'&depth=2', {headers: AUTH_HEADERS}).then(r=>r.json()).catch(()=>({})),
    ]);
    const nd = ctxR.node || {};
    // ── WHAT it is ──
    html += '<div class="what">';
    html += '<div class="kv"><span class="k">what</span><span class="v">'+esc(n.kind||'?')+(nd.mime?' · '+esc(nd.mime):'')+'</span></div>';
    if (nd.size) html += '<div class="kv"><span class="k">size</span><span class="v">'+nd.size+' B</span></div>';
    if (nd.content_hash) html += '<div class="kv"><span class="k">sha</span><span class="v">'+esc(String(nd.content_hash).slice(0,12))+'…</span></div>';
    if (ctxR.link_counts) {
      let pills=''; for (const [k,v] of Object.entries(ctxR.link_counts)) pills += '<span class="pill">'+esc(k)+':'+v+'</span>';
      if (pills) html += '<div style="margin-top:4px">'+pills+'</div>';
    }
    if (n.kind==='dir') html += '<div class="kv"><span class="k">kind</span><span class="v">block / directory'+(expanded.has(id)?' (expanded)':'')+'</span></div>';
    html += '</div>';
    // ── WHERE it is ──
    html += '<div class="where">';
    if (n.path) html += '<div class="kv"><span class="k">path</span><span class="v">'+esc(n.path)+'</span></div>';
    if (parentDir) html += '<div class="kv"><span class="k">parent</span><span class="v">'+esc(parentDir)+'</span></div>';
    if (nd.root) html += '<div class="kv"><span class="k">root</span><span class="v">'+esc(nd.root)+'</span></div>';
    html += '<div class="kv"><span class="k">id</span><span class="v">'+esc(id)+'</span></div>';
    html += '</div>';
    // ── actions ──
    if (n.kind==='dir') {
      const isExp = expanded.has(id);
      html += '<div style="margin:6px 0">'
        + '<span class="btn primary" onclick="toggleExpand('+js(id)+')">'+(isExp?'Collapse block':'Expand block')+'</span>'
        + '<span class="btn" onclick="locate('+js(id)+')">Center</span>'
        + '<span class="btn" onclick="focusOn('+js(id)+',2)">Zoom in</span>'
        + '</div>';
      html += '<div class="dim">'+(childCount[id]||0)+' direct children in this block</div>';
    } else {
      html += '<div style="margin:6px 0"><span class="btn" onclick="locate('+js(id)+')">Center</span><span class="btn" onclick="focusOn('+js(id)+',2.2)">Zoom in</span></div>';
    }
    // ── impact (blast radius) ──
    if (impR && (impR.direct || impR.transitive)) {
      const tot = impR.total_affected || 0;
      html += '<h3>impact ('+tot+' affected)</h3>';
      const d = (impR.direct||[]).slice(0, INSPECT_CAP);
      const t = (impR.transitive||[]).slice(0, INSPECT_CAP);
      if (d.length) html += '<div class="dim">direct ('+d.length+(impR.direct.length>INSPECT_CAP?' of '+impR.direct.length:'')+'):</div><ul>' + d.map(p=>'<li onclick="inspectNode('+js(p)+')">'+esc(p)+'</li>').join('') + '</ul>';
      if (t.length) html += '<div class="dim">transitive ('+t.length+(impR.transitive.length>INSPECT_CAP?' of '+impR.transitive.length:'')+'):</div><ul>' + t.map(p=>'<li onclick="inspectNode('+js(p)+')">'+esc(p)+'</li>').join('') + '</ul>';
    } else if (impR && impR.target) {
      html += '<h3>impact</h3><div class="dim">nothing depends on this node</div>';
    }
    // ── neighbors ──
    const nb = (nbR.neighbors||nbR.nodes||[]);
    if (nb.length) {
      const shown = nb.slice(0, INSPECT_CAP);
      html += '<h3>neighbors ('+shown.length+(nb.length>INSPECT_CAP?' of '+nb.length:'')+')</h3><ul>';
      for (const it of shown) { const p = it.id||it; html += '<li onclick="inspectNode('+js(p)+')">'+esc(p)+'</li>'; }
      if (nb.length > INSPECT_CAP) html += '<li class="dim">…'+(nb.length-INSPECT_CAP)+' more</li>';
      html += '</ul>';
    }
    // ── dangling references ──
    if (ctxR.dangling_references && ctxR.dangling_references.length) {
      html += '<h3>dangling refs</h3><ul>' + ctxR.dangling_references.slice(0, INSPECT_CAP).map(p=>'<li>'+esc(p)+'</li>').join('') + '</ul>';
    }
  } catch (err) { html = '<div class="dim">failed to load: '+esc(String(err))+'</div>'; }
  document.getElementById('p_body').innerHTML = html || '<div class="dim">no data</div>';
}

// ── SSE event stream (batched ingest, no per-event countEdges) ───────────────
if (typeof EventSource !== 'undefined') {
  // EventSource can't set headers, so the token rides in the query string.
  const es = new EventSource('/events?token='+encodeURIComponent(TOKEN));
  es.onmessage = e => {
    const ev = JSON.parse(e.data);
    if (ev.kind === 'node') {
      // Single node (Python fallback path or incremental re-crawl).
      if (!nodes[ev.id]) {
        nodes[ev.id] = {x: (Math.random()-0.5)*400, y: (Math.random()-0.5)*400, vx:0, vy:0, kind: ev.node_kind, path: ev.path, fixed:false, score:0};
        cnt.nodes++; QT.dirty = true;
      }
    } else if (ev.kind === 'nodes_batch') {
      // Bulk ingest (the common path for a structural crawl): add all at once,
      // one counter bump, one quadtree dirty flag — NOT a per-node countEdges.
      for (const nd of ev.nodes||[]) {
        if (!nodes[nd.id]) {
          nodes[nd.id] = {x: (Math.random()-0.5)*400, y: (Math.random()-0.5)*400, vx:0, vy:0, kind: nd.node_kind, path: nd.path, fixed:false, score:0};
        }
      }
      cnt.nodes = Object.keys(nodes).length; QT.dirty = true;
    } else if (ev.kind === 'edge') {
      const key = ev.src+'|'+ev.dst+'|'+ev.edge_type;
      if (!edges[key]) {
        edges[key] = {src:ev.src, dst:ev.dst, type:ev.edge_type, weight:ev.weight};
        cnt[ev.edge_type]++; QT.dirty=true;
        // Track the directory hierarchy so big dirs can collapse into blocks.
        if (ev.edge_type==='contains') {
          childCount[ev.src] = (childCount[ev.src] || 0) + 1;
          (childrenOf[ev.src] = childrenOf[ev.src] || []).push(ev.dst);
          parentOf[ev.dst] = ev.src;
        }
      }
    } else if (ev.kind === 'reset_dim') {
      for (const k in edges) if (edges[k].type===ev.edge_type) { delete edges[k]; }
      cnt[ev.edge_type]=0; reheat=3; QT.dirty=true;
      if (ev.edge_type==='contains') rebuildFamilyMaps();
    } else if (ev.kind === 'pass') {
      document.getElementById('v_pass').textContent = ev.name + (ev.status==='start'?' ▶':' ✓');
      if (ev.status==='start') reheat=4;
    } else if (ev.kind === 'cycle') {
      document.getElementById('v_cycle').textContent = ev.n;
      recomputeScores(); reheat=5;
    } else if (ev.kind === 'done') {
      document.getElementById('v_stat').textContent = ev.converged ? 'converged' : 'stopped';
      const st = document.getElementById('status');
      st.textContent = ev.converged ? 'converged' : 'done';
      st.className = 'badge ' + (ev.converged ? 'ok' : 'run');
      if (ev.fragments !== undefined) document.getElementById('n_frags').textContent = ev.fragments;
      recomputeScores();
      rebuildFamilyMaps();   // finalise block hierarchy now all contains edges are in
      centerView();  // center the graph in the viewport after convergence
    } else if (ev.kind === 'start') {
      document.getElementById('v_stat').textContent='running';
      document.getElementById('status').textContent='live';
      document.getElementById('status').className='badge run';
    } else if (ev.kind === 'dropped') {
      // Backpressure: the daemon dropped events because we were slow. Show it.
      dropped = ev.count;
      document.getElementById('v_stat').textContent = 'live (sampled: '+ev.count+' dropped)';
    } else if (ev.kind && ev.kind.indexOf('fs_')===0) {
      document.getElementById('v_stat').textContent='re-crawling…';
      document.getElementById('status').textContent='updating';
      document.getElementById('status').className='badge run';
      reheat=6;
    } else if (ev.kind === 'cross_links') {
      // Cross-dir links established between fragments.
      document.getElementById('v_stat').textContent='linked '+ev.count+' cross-dir refs';
    } else if (ev.kind === 'cross_dir_impact') {
      // A local change propagated across dir boundaries — flash the affected dirs.
      const frags = ev.fragments || [];
      const dirs = frags.map(f => (f.changed_dir||'').split(/[\\\\/]/).pop() + ' → [' +
                    (f.affected_dirs||[]).map(d => d.split(/[\\\\/]/).pop()).join(', ') + ']');
      document.getElementById('v_stat').textContent='cross-dir impact: ' + dirs.join('; ');
      reheat=8;
    }
    updateCounters();
  };
}

tick();
</script>

</body>
</html>
'''


class TerminalReporter:
    """Consumes the SSE event stream and prints crawl progress to the terminal.

    Used by ``dataworm crawl --live`` (the ``--live`` flag opens its own SSE
    connection to the daemon and feeds events here).
    """

    def __init__(self):
        self.nodes = 0
        self.edges = 0
        self.current_cycle = 0
        self.current_pass = None
        self.current_status = "starting"

    def __call__(self, event):
        k = event.get("kind")
        if k == "node":
            self.nodes += 1
            # Print every 25 nodes to keep terminal readable.
            if self.nodes % 25 == 0:
                print(f"  ... {self.nodes} nodes discovered (live)", flush=True)
        elif k == "edge":
            t = event.get("edge_type", "?")
            self.edges += 1
            print(f"  + edge {t}: {event.get('src')} -> {event.get('dst')}  (cycle {self.current_cycle})", flush=True)
        elif k == "pass":
            name = event.get("name", "?")
            cycle = event.get("cycle", 0)
            status = event.get("status")
            self.current_cycle = cycle
            self.current_pass = name
            self.current_status = status
            label = "start" if status == "start" else "end"
            print(f"[cycle {cycle}] pass '{name}' => {label}", flush=True)
        elif k == "reset_dim":
            print(f"  ~ reset dimension '{event.get('edge_type')}' ({event.get('removed', '?')} edges removed for recompute)", flush=True)
        elif k == "cycle":
            print(f"  cycle {event.get('n')} complete (sig changed? converging...)", flush=True)
        elif k == "done":
            print(f"\n=== DONE === converged={event.get('converged')} cycles={event.get('cycles')}")
            c = event.get("counts", {})
            print(f"nodes={c.get('nodes', '?')}  contains={c.get('edges_contains', '?')}")
            print(f"references={c.get('edges_references', '?')}  duplicate_of={c.get('edges_duplicate_of', '?')}")
            print(f"similar_to={c.get('edges_similar_to', '?')}")
            print(f"graph saved (see --db to query)")
            self.current_status = "done"
        elif k and k.startswith("fs_"):
            # fs_event signals from the watcher / incremental re-crawl.
            print(f"  [watch] {k}: {event.get('path', '')}", flush=True)
