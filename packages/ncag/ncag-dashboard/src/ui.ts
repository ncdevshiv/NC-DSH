/**
 * The dashboard document served at the control plane root. It renders only data
 * the API returned and every control calls a documented API route.
 * @module @deepseek-ai/dsh-ncag-dashboard/ui
 */

/** The complete control-plane document, shipped as one server-rendered page. */
export const DASHBOARD_HTML = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>NC-AG Control Plane</title>
<style>
:root { color-scheme: dark light; --bg:#0b0d10; --panel:#141821; --line:#232a36; --ink:#e6e9ef; --muted:#96a0b0; --accent:#4c8dff; --ok:#31b56b; --warn:#e0a651; --bad:#e2564d; }
@media (prefers-color-scheme: light) { :root { --bg:#f7f8fa; --panel:#ffffff; --line:#dfe3ea; --ink:#161a20; --muted:#5b6676; } }
* { box-sizing: border-box; }
body { margin:0; font:14px/1.45 ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif; background:var(--bg); color:var(--ink); }
header { display:flex; gap:16px; align-items:center; flex-wrap:wrap; padding:12px 20px; border-bottom:1px solid var(--line); position:sticky; top:0; background:var(--bg); z-index:2; }
h1 { font-size:16px; margin:0; letter-spacing:.02em; }
.badge { font-size:12px; padding:2px 8px; border-radius:999px; border:1px solid var(--line); color:var(--muted); }
main { display:grid; grid-template-columns:minmax(280px, 1fr) minmax(320px, 1.4fr); gap:16px; padding:16px 20px 40px; }
@media (max-width: 900px) { main { grid-template-columns:1fr; } }
section { background:var(--panel); border:1px solid var(--line); border-radius:10px; overflow:hidden; }
section h2 { font-size:13px; text-transform:uppercase; letter-spacing:.08em; color:var(--muted); margin:0; padding:10px 14px; border-bottom:1px solid var(--line); }
.pad { padding:12px 14px; }
.row { display:flex; justify-content:space-between; gap:12px; padding:6px 0; border-bottom:1px dashed var(--line); }
.row:last-child { border-bottom:0; }
.k { color:var(--muted); }
ul.list { margin:0; padding:0; list-style:none; max-height:320px; overflow:auto; }
ul.list li { padding:8px 14px; border-bottom:1px solid var(--line); display:flex; flex-direction:column; gap:2px; }
ul.list li:last-child { border-bottom:0; }
.small { font-size:12px; color:var(--muted); }
.tag { font-size:11px; padding:1px 6px; border-radius:4px; background:var(--line); }
.tag.ok { background:rgba(49,181,107,.18); color:var(--ok); }
.tag.bad { background:rgba(226,86,77,.18); color:var(--bad); }
.tag.warn { background:rgba(224,166,81,.18); color:var(--warn); }
button { font:inherit; padding:5px 10px; border-radius:6px; border:1px solid var(--line); background:transparent; color:var(--ink); cursor:pointer; }
button:hover { border-color:var(--accent); }
button.primary { background:var(--accent); border-color:var(--accent); color:#fff; }
button:disabled { opacity:.5; cursor:not-allowed; }
input, textarea { width:100%; font:inherit; padding:6px 8px; border-radius:6px; border:1px solid var(--line); background:var(--bg); color:var(--ink); }
textarea { min-height:60px; resize:vertical; }
.form { display:flex; flex-direction:column; gap:8px; }
.form label { font-size:12px; color:var(--muted); }
pre { margin:0; padding:10px 14px; font-size:12px; overflow:auto; max-height:280px; }
#error { color:var(--bad); font-size:12px; padding:0 14px 10px; }
</style>
</head>
<body>
<header>
  <h1>NC-AG Control Plane</h1>
  <span class="badge" id="health">checking health</span>
  <span class="badge" id="stream">stream disconnected</span>
  <span style="flex:1"></span>
  <span class="small" id="generated"></span>
</header>
<main>
  <div>
    <section>
      <h2>Projects</h2>
      <ul class="list" id="projects"><li class="small pad">no project yet</li></ul>
      <div class="pad form">
        <label for="project-name">New project name</label>
        <input id="project-name" placeholder="Project name" />
        <label for="project-description">Description</label>
        <input id="project-description" placeholder="What the project delivers" />
        <label for="project-repository">Repository path</label>
        <input id="project-repository" placeholder="Absolute path of a git repository" />
        <button class="primary" id="create-project">Create project</button>
        <div id="error" role="alert"></div>
      </div>
    </section>
    <section style="margin-top:16px">
      <h2>Submit a request</h2>
      <div class="pad form">
        <label for="request-project">Project</label>
        <select id="request-project"></select>
        <label for="request-text">Request</label>
        <textarea id="request-text" placeholder="Describe the work"></textarea>
        <button class="primary" id="submit-request">Submit request</button>
      </div>
    </section>
    <section style="margin-top:16px">
      <h2>Pending decisions</h2>
      <ul class="list" id="decisions"><li class="small pad">no pending decision</li></ul>
    </section>
  </div>
  <div>
    <section>
      <h2>Requirements</h2>
      <ul class="list" id="requirements"><li class="small pad">no requirement yet</li></ul>
    </section>
    <section style="margin-top:16px">
      <h2>Tasks</h2>
      <ul class="list" id="tasks"><li class="small pad">no task yet</li></ul>
    </section>
    <section style="margin-top:16px">
      <h2>Agents</h2>
      <ul class="list" id="agents"><li class="small pad">no agent yet</li></ul>
    </section>
    <section style="margin-top:16px">
      <h2>Resource leases</h2>
      <ul class="list" id="leases"><li class="small pad">no active lease</li></ul>
      <h2 style="border-top:1px solid var(--line)">Waiting agents</h2>
      <ul class="list" id="waiters"><li class="small pad">no waiting agent</li></ul>
    </section>
    <section style="margin-top:16px">
      <h2>Usage and cost</h2>
      <div class="pad" id="usage"></div>
    </section>
    <section style="margin-top:16px">
      <h2>Timeline</h2>
      <ul class="list" id="events"><li class="small pad">no event yet</li></ul>
    </section>
    <section style="margin-top:16px">
      <h2>Review runs</h2>
      <ul class="list" id="reviews"><li class="small pad">no review yet</li></ul>
    </section>
    <section style="margin-top:16px">
      <h2>Communication</h2>
      <div class="pad form">
        <label for="message-filter">Filter messages</label>
        <input id="message-filter" placeholder="type or text" />
      </div>
      <ul class="list" id="messages"><li class="small pad">no message yet</li></ul>
    </section>
    <section style="margin-top:16px">
      <h2>Decision history</h2>
      <div class="pad form">
        <label for="decision-filter">Filter decisions</label>
        <input id="decision-filter" placeholder="status or problem" />
      </div>
      <ul class="list" id="decisions-history"><li class="small pad">no decision yet</li></ul>
    </section>
    <section style="margin-top:16px">
      <h2>Audit chain</h2>
      <div class="pad" id="audit"></div>
    </section>
  </div>
</main>
<script>
const state = { projectId: null, snapshot: null };
const token = new URLSearchParams(location.search).get('token');
function api(path, options) {
  const headers = { 'content-type': 'application/json' };
  if (token) headers.authorization = 'Bearer ' + token;
  return fetch(path, Object.assign({ headers }, options || {})).then(async response => {
    const text = await response.text();
    const body = text ? JSON.parse(text) : null;
    if (!response.ok) throw new Error((body && body.error) || ('HTTP ' + response.status));
    return body;
  });
}
function showError(message) { document.getElementById('error').textContent = message; }
function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}
function statusTag(status) {
  const ok = ['COMPLETED', 'IDLE', 'ACTIVE', 'MERGED'];
  const bad = ['FAILED', 'REJECTED', 'CONFLICT', 'CANCELLED'];
  const warn = ['BLOCKED', 'PAUSED', 'REVIEW_PENDING', 'VERIFICATION_PENDING', 'WAITING', 'HELP_REQUESTED'];
  const cls = ok.includes(status) ? 'tag ok' : bad.includes(status) ? 'tag bad' : warn.includes(status) ? 'tag warn' : 'tag';
  return el('span', cls, status);
}
async function refresh() {
  const snapshot = await api('/api/snapshot');
  state.snapshot = snapshot;
  document.getElementById('generated').textContent = 'snapshot ' + snapshot.generatedAt;
  document.getElementById('health').textContent = snapshot.audit.brokenAt === null ? 'audit chain verified' : 'audit chain broken at ' + snapshot.audit.brokenAt;
  document.getElementById('health').className = 'badge';
  renderProjects(snapshot);
  renderUsage(snapshot);
  renderEvents(snapshot.events);
  document.getElementById('audit').textContent = 'records ' + snapshot.audit.records + ', brokenAt ' + snapshot.audit.brokenAt + ', routes ' + snapshot.routes.length + ', profiles ' + snapshot.profiles;
  if (state.projectId) await refreshProject(state.projectId);
}
function renderProjects(snapshot) {
  const list = document.getElementById('projects');
  const select = document.getElementById('request-project');
  list.innerHTML = '';
  select.innerHTML = '';
  if (snapshot.projects.length === 0) { list.appendChild(el('li', 'small pad', 'no project yet')); return; }
  for (const entry of snapshot.projects) {
    const item = el('li');
    const head = el('div', 'row');
    head.appendChild(el('span', '', entry.project.name));
    head.appendChild(statusTag(entry.project.status));
    item.appendChild(head);
    item.appendChild(el('div', 'small', entry.project.id));
    item.appendChild(el('div', 'small', entry.requests + ' request(s), ' + entry.tasks.length + ' task(s), ' + entry.agents.length + ' agent(s), ' + entry.leases + ' lease(s), $' + entry.costUsd.toFixed(4)));
    if (entry.decisionsPending > 0) item.appendChild(el('div', 'small', entry.decisionsPending + ' pending decision(s)'));
    const actions = el('div', 'row');
    const open = el('button', '', 'Inspect');
    open.onclick = () => { state.projectId = entry.project.id; refreshProject(entry.project.id); };
    const pause = el('button', '', entry.project.status === 'PAUSED' ? 'Resume' : 'Pause');
    pause.onclick = async () => {
      try {
        await api('/api/projects/' + entry.project.id + (entry.project.status === 'PAUSED' ? '/resume' : '/pause'), { method: 'POST', body: JSON.stringify({ reason: 'operator request' }) });
        await refresh();
      } catch (error) { showError(error.message); }
    };
    actions.appendChild(open);
    actions.appendChild(pause);
    item.appendChild(actions);
    list.appendChild(item);
    const option = el('option', '', entry.project.name);
    option.value = entry.project.id;
    select.appendChild(option);
    if (state.projectId === null) state.projectId = entry.project.id;
  }
  select.value = state.projectId;
}
async function refreshProject(projectId) {
  const project = state.snapshot.projects.find(entry => entry.project.id === projectId);
  if (!project) return;
  await Promise.all([renderTasks(projectId), renderAgents(projectId), renderLeases(projectId), renderWaiters(projectId), renderDecisions(projectId), renderMessages(projectId), renderDecisionHistory(projectId), renderRequirements(projectId), renderReviews(projectId)]);
}
async function renderTasks(projectId) {
  const tasks = await api('/api/projects/' + projectId + '/tasks');
  const list = document.getElementById('tasks');
  list.innerHTML = '';
  if (tasks.length === 0) { list.appendChild(el('li', 'small pad', 'no task yet')); return; }
  for (const task of tasks) {
    const item = el('li');
    const head = el('div', 'row');
    head.appendChild(el('span', '', task.title));
    head.appendChild(statusTag(task.status));
    item.appendChild(head);
    item.appendChild(el('div', 'small', task.requiredCapability + ' | ' + task.priority + ' | depends on ' + (task.dependencyIds.length || 'nothing')));
    if (task.statusReason) item.appendChild(el('div', 'small', 'reason: ' + task.statusReason));
    const actions = el('div', 'row');
    const cancel = el('button', '', 'Cancel');
    cancel.onclick = async () => {
      try { await api('/api/tasks/' + task.id + '/cancel', { method: 'POST', body: JSON.stringify({ reason: 'operator request' }) }); await refresh(); }
      catch (error) { showError(error.message); }
    };
    const inspect = el('button', '', 'Evidence');
    inspect.onclick = () => toggleEvidence(item, task.id);
    actions.appendChild(inspect);
    actions.appendChild(cancel);
    item.appendChild(actions);
    list.appendChild(item);
  }
}
async function renderAgents(projectId) {
  const agents = await api('/api/projects/' + projectId + '/agents');
  const list = document.getElementById('agents');
  list.innerHTML = '';
  if (agents.length === 0) { list.appendChild(el('li', 'small pad', 'no agent yet')); return; }
  for (const agent of agents) {
    const item = el('li');
    const head = el('div', 'row');
    head.appendChild(el('span', '', agent.profileKey));
    head.appendChild(statusTag(agent.status));
    item.appendChild(head);
    item.appendChild(el('div', 'small', 'task ' + (agent.taskId || 'none') + ' | model ' + agent.model.provider + '/' + agent.model.model + ' | steps ' + agent.steps));
    item.appendChild(el('div', 'small', 'workspace ' + (agent.workspacePath || 'none') + ' | branch ' + (agent.branch || 'none')));
    item.appendChild(el('div', 'small', 'usage ' + agent.usage.inputTokens + ' in / ' + agent.usage.outputTokens + ' out | 
async function renderWaiters(projectId) {
  const waiters = await api('/api/projects/' + projectId + '/waiters');
  const list = document.getElementById('waiters');
  list.innerHTML = '';
  if (waiters.length === 0) { list.appendChild(el('li', 'small pad', 'no waiting agent')); return; }
  for (const waiter of waiters) {
    const item = el('li');
    item.appendChild(el('div', 'row', waiter.resourcePath + ' (' + waiter.mode + ')'));
    item.appendChild(el('div', 'small', 'agent ' + waiter.agentId + ' on task ' + (waiter.taskId || 'none') + ' since ' + waiter.requestedAt));
    item.appendChild(el('div', 'small', 'reason: ' + waiter.reason));
    list.appendChild(item);
  }
}
async function renderRequirements(projectId) {
  const [requirements, requests] = await Promise.all([
    api('/api/projects/' + projectId + '/requirements'),
    api('/api/projects/' + projectId + '/requests'),
  ]);
  const list = document.getElementById('requirements');
  list.innerHTML = '';
  if (requests.length === 0 && requirements.length === 0) { list.appendChild(el('li', 'small pad', 'no request yet')); return; }
  for (const request of requests.slice().reverse()) {
    const item = el('li');
    item.appendChild(el('div', 'row', request.rawText.slice(0, 120)));
    item.appendChild(el('div', 'small', request.id + ' | ' + request.status + ' | ' + request.requirements.length + ' requirement(s)'));
    list.appendChild(item);
  }
  for (const requirement of requirements) {
    const item = el('li');
    const head = el('div', 'row');
    head.appendChild(el('span', '', requirement.description.slice(0, 100)));
    head.appendChild(statusTag(requirement.status));
    item.appendChild(head);
    item.appendChild(el('div', 'small', requirement.category + ' | ' + requirement.priority + ' | from ' + requirement.source));
    for (const criterion of requirement.acceptanceCriteria) item.appendChild(el('div', 'small', 'criterion ' + criterion.id + ' (' + criterion.verification + '): ' + criterion.text));
    list.appendChild(item);
  }
}
async function renderReviews(projectId) {
  const runs = await api('/api/projects/' + projectId + '/reviews');
  const list = document.getElementById('reviews');
  list.innerHTML = '';
  if (runs.length === 0) { list.appendChild(el('li', 'small pad', 'no review yet')); return; }
  for (const run of runs.slice().reverse()) {
    const item = el('li');
    const head = el('div', 'row');
    head.appendChild(el('span', '', run.reviewKind + ' by ' + run.reviewerAgentId));
    head.appendChild(statusTag(run.rejected ? 'REJECTED' : 'ACCEPTED'));
    item.appendChild(head);
    item.appendChild(el('div', 'small', run.summary + ' | ' + run.findings + ' finding(s) | task ' + run.taskId));
    item.appendChild(el('div', 'small', 'at ' + run.createdAt));
    list.appendChild(item);
  }
}

async function renderLeases(projectId) {
  const leases = await api('/api/projects/' + projectId + '/leases');
  const list = document.getElementById('leases');
  list.innerHTML = '';
  if (leases.length === 0) { list.appendChild(el('li', 'small pad', 'no active lease')); return; }
  for (const lease of leases) {
    const item = el('li');
    item.appendChild(el('div', 'row', lease.resourcePath));
    item.appendChild(el('div', 'small', lease.mode + ' lease held by ' + lease.agentId + ' on task ' + (lease.taskId || 'none') + ' until ' + new Date(lease.expiresAtMs).toISOString() + ' (fencing token ' + lease.fencingToken + ')'))
    item.appendChild(el('div', 'small', 'reason: ' + lease.reason));
    list.appendChild(item);
  }
}
async function renderDecisions(projectId) {
  const decisions = await api('/api/projects/' + projectId + '/decisions');
  const list = document.getElementById('decisions');
  list.innerHTML = '';
  const pending = decisions.filter(decision => decision.status === 'AWAITING_USER');
  if (pending.length === 0) { list.appendChild(el('li', 'small pad', 'no pending decision')); return; }
  for (const decision of pending) {
    const item = el('li');
    item.appendChild(el('div', 'row', decision.problem));
    if (decision.debate) {
      const debate = decision.debate;
      item.appendChild(el('div', 'small', 'requested: ' + debate.requested));
      item.appendChild(el('div', 'small', 'belief: ' + debate.belief));
      item.appendChild(el('div', 'small', 'conflict: ' + debate.conflict));
      item.appendChild(el('div', 'small', 'recommendation: ' + debate.recommendation));
      for (const alternative of debate.alternatives) {
        item.appendChild(el('div', 'small', alternative.label + ': +' + alternative.advantages.join('; ') + ' | -' + alternative.disadvantages.join('; ')));
      }
      item.appendChild(el('div', 'small', 'evidence: ' + debate.evidence.join('; ')));
      item.appendChild(el('div', 'small', 'question: ' + debate.question));
    }
    const actions = el('div', 'row');
    const accept = el('button', 'primary', 'Accept recommendation');
    accept.onclick = async () => {
      try { await api('/api/decisions/' + decision.id + '/resolve', { method: 'POST', body: JSON.stringify({ outcome: decision.recommendation }) }); await refresh(); }
      catch (error) { showError(error.message); }
    };
    const override = el('button', '', 'Record alternative');
    override.onclick = async () => {
      const outcome = prompt('Record the chosen approach', decision.debate ? decision.debate.requested : decision.recommendation);
      if (outcome === null) return;
      const rationale = prompt('Record the rationale', '');
      try { await api('/api/decisions/' + decision.id + '/resolve', { method: 'POST', body: JSON.stringify({ outcome, rationale: rationale || null }) }); await refresh(); }
      catch (error) { showError(error.message); }
    };
    actions.appendChild(accept);
    actions.appendChild(override);
    item.appendChild(actions);
    list.appendChild(item);
  }
}
function renderUsage(snapshot) {
  const node = document.getElementById('usage');
  node.innerHTML = '';
  const rows = [
    ['Input tokens', String(snapshot.usage.inputTokens)],
    ['Output tokens', String(snapshot.usage.outputTokens)],
    ['Model calls', String(snapshot.usage.calls)],
    ['Cost (USD)', snapshot.usage.costUsd.toFixed(4)],
    ['Model routes', String(snapshot.routes.length)],
  ];
  for (const [label, value] of rows) {
    const row = el('div', 'row');
    row.appendChild(el('span', 'k', label));
    row.appendChild(el('span', '', value));
    node.appendChild(row);
  }
}
function renderEvents(events) {
  const list = document.getElementById('events');
  list.innerHTML = '';
  const recent = events.slice(-40).reverse();
  if (recent.length === 0) { list.appendChild(el('li', 'small pad', 'no event yet')); return; }
  for (const event of recent) {
    const item = el('li');
    item.appendChild(el('div', '', event.summary));
    item.appendChild(el('div', 'small', event.at + ' | ' + event.kind));
    list.appendChild(item);
  }
}

async function toggleEvidence(item, taskId) {
  const existing = item.querySelector('.evidence');
  if (existing) { existing.remove(); return; }
  const box = el('div', 'evidence');
  try {
    const [changes, evidence, findings] = await Promise.all([
      api('/api/tasks/' + taskId + '/changesets'),
      api('/api/tasks/' + taskId + '/evidence'),
      api('/api/tasks/' + taskId + '/findings'),
    ]);
    box.appendChild(el('div', 'small', 'changed files: ' + (changes.reduce((all, entry) => all.concat(entry.changedFiles || []), []).join(', ') || 'none')));
    for (const entry of evidence) {
      box.appendChild(el('div', 'small', entry.criterionId + ' [' + entry.status + '] ' + (entry.command || entry.method) + ' exit ' + (entry.exitCode === null ? 'n/a' : entry.exitCode)));
      box.appendChild(el('pre', 'small', (entry.output || '').slice(0, 400)));
    }
    if (evidence.length === 0) box.appendChild(el('div', 'small', 'no verification evidence recorded'));
    for (const finding of findings) {
      box.appendChild(el('div', 'small', finding.severity + ' [' + finding.status + '] ' + finding.artifact + ': ' + finding.description));
    }
    if (findings.length === 0) box.appendChild(el('div', 'small', 'no review finding recorded'));
  } catch (error) { box.appendChild(el('div', 'small', 'evidence unavailable: ' + error.message)); }
  item.appendChild(box);
}
async function renderMessages(projectId) {
  const messages = await api('/api/projects/' + projectId + '/messages');
  const list = document.getElementById('messages');
  const filter = document.getElementById('message-filter').value.toLowerCase();
  list.innerHTML = '';
  const visible = messages.filter(message => filter.length === 0 || (message.type + ' ' + message.subject + ' ' + message.content).toLowerCase().includes(filter));
  if (visible.length === 0) { list.appendChild(el('li', 'small pad', messages.length === 0 ? 'no message yet' : 'no message matches the filter')); return; }
  for (const message of visible.slice().reverse()) {
    const item = el('li');
    const head = el('div', 'row');
    head.appendChild(el('span', '', message.type + ' ' + message.subject));
    head.appendChild(el('span', 'small', message.deliveryStatus));
    item.appendChild(head);
    item.appendChild(el('div', 'small', message.timestamp + ' | ' + (message.target.kind === 'agent' ? 'agent ' + message.target.agentId : message.target.kind === 'channel' ? message.target.channel : 'orchestrator')));
    item.appendChild(el('div', 'small', message.content.slice(0, 300)));
    list.appendChild(item);
  }
}
async function renderDecisionHistory(projectId) {
  const decisions = await api('/api/projects/' + projectId + '/decisions');
  const list = document.getElementById('decisions-history');
  const filter = document.getElementById('decision-filter').value.toLowerCase();
  list.innerHTML = '';
  const visible = decisions.filter(decision => filter.length === 0 || (decision.status + ' ' + decision.problem + ' ' + decision.outcome).toLowerCase().includes(filter));
  if (visible.length === 0) { list.appendChild(el('li', 'small pad', decisions.length === 0 ? 'no decision yet' : 'no decision matches the filter')); return; }
  for (const decision of visible.slice().reverse()) {
    const item = el('li');
    const head = el('div', 'row');
    head.appendChild(el('span', '', decision.problem));
    head.appendChild(statusTag(decision.status));
    item.appendChild(head);
    item.appendChild(el('div', 'small', decision.category + ' | recommended: ' + decision.recommendation));
    item.appendChild(el('div', 'small', 'outcome: ' + (decision.outcome || '(pending)') + ' | rationale: ' + (decision.rationale || 'not stated')));
    if (decision.rejectedRecommendation) item.appendChild(el('div', 'small', 'rejected: ' + decision.rejectedRecommendation));
    for (const consequence of decision.consequences || []) item.appendChild(el('div', 'small', 'consequence: ' + consequence));
    list.appendChild(item);
  }
}

document.getElementById('create-project').onclick = async () => {
  try {
    const project = await api('/api/projects', { method: 'POST', body: JSON.stringify({
      name: document.getElementById('project-name').value,
      description: document.getElementById('project-description').value,
      repositoryPath: document.getElementById('project-repository').value,
    }) });
    state.projectId = project.id;
    showError('');
    await refresh();
  } catch (error) { showError(error.message); }
};
document.getElementById('submit-request').onclick = async () => {
  try {
    const projectId = document.getElementById('request-project').value;
    const text = document.getElementById('request-text').value;
    await api('/api/projects/' + projectId + '/requests', { method: 'POST', body: JSON.stringify({ text }) });
    showError('');
    await refresh();
  } catch (error) { showError(error.message); }
};
for (const id of ['message-filter', 'decision-filter']) {
  document.getElementById(id).addEventListener('input', () => {
    if (!state.projectId) return;
    if (id === 'message-filter') void renderMessages(state.projectId);
    else void renderDecisionHistory(state.projectId);
  });
}

const stream = new EventSource('/events' + (token ? '?token=' + encodeURIComponent(token) : ''));
stream.onopen = () => { document.getElementById('stream').textContent = 'stream connected'; };
stream.onerror = () => { document.getElementById('stream').textContent = 'stream reconnecting'; };
stream.onmessage = () => { void refresh(); };
refresh().catch(error => showError(error.message));
</script>
</body>
</html> + agent.usage.costUsd.toFixed(4) + ' | failures ' + agent.failureCount));
    item.appendChild(el('div', 'small', 'created ' + agent.createdAt + ' | heartbeat ' + agent.lastHeartbeatAt + ' | activity ' + agent.lastActivityAt));
    if (agent.statusReason) item.appendChild(el('div', 'small', 'reason: ' + agent.statusReason));
    const actions = el('div', 'row');
    const input = el('input');
    input.placeholder = 'instruction';
    const send = el('button', '', 'Instruct');
    send.onclick = async () => {
      try { await api('/api/agents/' + agent.id + '/instruct', { method: 'POST', body: JSON.stringify({ text: input.value }) }); input.value = ''; await refresh(); }
      catch (error) { showError(error.message); }
    };
    const cancel = el('button', '');
    cancel.textContent = 'Cancel agent';
    cancel.onclick = async () => {
      try { await api('/api/agents/' + agent.id + '/cancel', { method: 'POST', body: JSON.stringify({ reason: 'operator request' }) }); await refresh(); }
      catch (error) { showError(error.message); }
    };
    const reassign = el('button', '', 'Reassign task');
    reassign.onclick = async () => {
      if (!agent.taskId) { showError('this agent holds no task to reassign'); return; }
      const capability = prompt('Capability that should take the task over', 'general-implementation');
      if (capability === null || capability.trim().length === 0) return;
      try { await api('/api/tasks/' + agent.taskId + '/reassign', { method: 'POST', body: JSON.stringify({ capability, reason: 'operator reassignment' }) }); await refresh(); }
      catch (error) { showError(error.message); }
    };
    actions.appendChild(input);
    actions.appendChild(send);
    actions.appendChild(cancel);
    actions.appendChild(reassign);
    item.appendChild(actions);
    list.appendChild(item);
  }
}
async function renderLeases(projectId) {
  const leases = await api('/api/projects/' + projectId + '/leases');
  const list = document.getElementById('leases');
  list.innerHTML = '';
  if (leases.length === 0) { list.appendChild(el('li', 'small pad', 'no active lease')); return; }
  for (const lease of leases) {
    const item = el('li');
    item.appendChild(el('div', 'row', lease.resourcePath));
    item.appendChild(el('div', 'small', lease.mode + ' lease held by ' + lease.agentId + ' until ' + new Date(lease.expiresAtMs).toISOString() + ' (fencing token ' + lease.fencingToken + ')'));
    list.appendChild(item);
  }
}
async function renderDecisions(projectId) {
  const decisions = await api('/api/projects/' + projectId + '/decisions');
  const list = document.getElementById('decisions');
  list.innerHTML = '';
  const pending = decisions.filter(decision => decision.status === 'AWAITING_USER');
  if (pending.length === 0) { list.appendChild(el('li', 'small pad', 'no pending decision')); return; }
  for (const decision of pending) {
    const item = el('li');
    item.appendChild(el('div', 'row', decision.problem));
    if (decision.debate) {
      const debate = decision.debate;
      item.appendChild(el('div', 'small', 'requested: ' + debate.requested));
      item.appendChild(el('div', 'small', 'belief: ' + debate.belief));
      item.appendChild(el('div', 'small', 'conflict: ' + debate.conflict));
      item.appendChild(el('div', 'small', 'recommendation: ' + debate.recommendation));
      for (const alternative of debate.alternatives) {
        item.appendChild(el('div', 'small', alternative.label + ': +' + alternative.advantages.join('; ') + ' | -' + alternative.disadvantages.join('; ')));
      }
      item.appendChild(el('div', 'small', 'evidence: ' + debate.evidence.join('; ')));
      item.appendChild(el('div', 'small', 'question: ' + debate.question));
    }
    const actions = el('div', 'row');
    const accept = el('button', 'primary', 'Accept recommendation');
    accept.onclick = async () => {
      try { await api('/api/decisions/' + decision.id + '/resolve', { method: 'POST', body: JSON.stringify({ outcome: decision.recommendation }) }); await refresh(); }
      catch (error) { showError(error.message); }
    };
    const override = el('button', '', 'Record alternative');
    override.onclick = async () => {
      const outcome = prompt('Record the chosen approach', decision.debate ? decision.debate.requested : decision.recommendation);
      if (outcome === null) return;
      const rationale = prompt('Record the rationale', '');
      try { await api('/api/decisions/' + decision.id + '/resolve', { method: 'POST', body: JSON.stringify({ outcome, rationale: rationale || null }) }); await refresh(); }
      catch (error) { showError(error.message); }
    };
    actions.appendChild(accept);
    actions.appendChild(override);
    item.appendChild(actions);
    list.appendChild(item);
  }
}
function renderUsage(snapshot) {
  const node = document.getElementById('usage');
  node.innerHTML = '';
  const rows = [
    ['Input tokens', String(snapshot.usage.inputTokens)],
    ['Output tokens', String(snapshot.usage.outputTokens)],
    ['Model calls', String(snapshot.usage.calls)],
    ['Cost (USD)', snapshot.usage.costUsd.toFixed(4)],
    ['Model routes', String(snapshot.routes.length)],
  ];
  for (const [label, value] of rows) {
    const row = el('div', 'row');
    row.appendChild(el('span', 'k', label));
    row.appendChild(el('span', '', value));
    node.appendChild(row);
  }
}
function renderEvents(events) {
  const list = document.getElementById('events');
  list.innerHTML = '';
  const recent = events.slice(-40).reverse();
  if (recent.length === 0) { list.appendChild(el('li', 'small pad', 'no event yet')); return; }
  for (const event of recent) {
    const item = el('li');
    item.appendChild(el('div', '', event.summary));
    item.appendChild(el('div', 'small', event.at + ' | ' + event.kind));
    list.appendChild(item);
  }
}

async function toggleEvidence(item, taskId) {
  const existing = item.querySelector('.evidence');
  if (existing) { existing.remove(); return; }
  const box = el('div', 'evidence');
  try {
    const [changes, evidence, findings] = await Promise.all([
      api('/api/tasks/' + taskId + '/changesets'),
      api('/api/tasks/' + taskId + '/evidence'),
      api('/api/tasks/' + taskId + '/findings'),
    ]);
    box.appendChild(el('div', 'small', 'changed files: ' + (changes.reduce((all, entry) => all.concat(entry.changedFiles || []), []).join(', ') || 'none')));
    for (const entry of evidence) {
      box.appendChild(el('div', 'small', entry.criterionId + ' [' + entry.status + '] ' + (entry.command || entry.method) + ' exit ' + (entry.exitCode === null ? 'n/a' : entry.exitCode)));
      box.appendChild(el('pre', 'small', (entry.output || '').slice(0, 400)));
    }
    if (evidence.length === 0) box.appendChild(el('div', 'small', 'no verification evidence recorded'));
    for (const finding of findings) {
      box.appendChild(el('div', 'small', finding.severity + ' [' + finding.status + '] ' + finding.artifact + ': ' + finding.description));
    }
    if (findings.length === 0) box.appendChild(el('div', 'small', 'no review finding recorded'));
  } catch (error) { box.appendChild(el('div', 'small', 'evidence unavailable: ' + error.message)); }
  item.appendChild(box);
}
async function renderMessages(projectId) {
  const messages = await api('/api/projects/' + projectId + '/messages');
  const list = document.getElementById('messages');
  const filter = document.getElementById('message-filter').value.toLowerCase();
  list.innerHTML = '';
  const visible = messages.filter(message => filter.length === 0 || (message.type + ' ' + message.subject + ' ' + message.content).toLowerCase().includes(filter));
  if (visible.length === 0) { list.appendChild(el('li', 'small pad', messages.length === 0 ? 'no message yet' : 'no message matches the filter')); return; }
  for (const message of visible.slice().reverse()) {
    const item = el('li');
    const head = el('div', 'row');
    head.appendChild(el('span', '', message.type + ' ' + message.subject));
    head.appendChild(el('span', 'small', message.deliveryStatus));
    item.appendChild(head);
    item.appendChild(el('div', 'small', message.timestamp + ' | ' + (message.target.kind === 'agent' ? 'agent ' + message.target.agentId : message.target.kind === 'channel' ? message.target.channel : 'orchestrator')));
    item.appendChild(el('div', 'small', message.content.slice(0, 300)));
    list.appendChild(item);
  }
}
async function renderDecisionHistory(projectId) {
  const decisions = await api('/api/projects/' + projectId + '/decisions');
  const list = document.getElementById('decisions-history');
  const filter = document.getElementById('decision-filter').value.toLowerCase();
  list.innerHTML = '';
  const visible = decisions.filter(decision => filter.length === 0 || (decision.status + ' ' + decision.problem + ' ' + decision.outcome).toLowerCase().includes(filter));
  if (visible.length === 0) { list.appendChild(el('li', 'small pad', decisions.length === 0 ? 'no decision yet' : 'no decision matches the filter')); return; }
  for (const decision of visible.slice().reverse()) {
    const item = el('li');
    const head = el('div', 'row');
    head.appendChild(el('span', '', decision.problem));
    head.appendChild(statusTag(decision.status));
    item.appendChild(head);
    item.appendChild(el('div', 'small', decision.category + ' | recommended: ' + decision.recommendation));
    item.appendChild(el('div', 'small', 'outcome: ' + (decision.outcome || '(pending)') + ' | rationale: ' + (decision.rationale || 'not stated')));
    if (decision.rejectedRecommendation) item.appendChild(el('div', 'small', 'rejected: ' + decision.rejectedRecommendation));
    for (const consequence of decision.consequences || []) item.appendChild(el('div', 'small', 'consequence: ' + consequence));
    list.appendChild(item);
  }
}

document.getElementById('create-project').onclick = async () => {
  try {
    const project = await api('/api/projects', { method: 'POST', body: JSON.stringify({
      name: document.getElementById('project-name').value,
      description: document.getElementById('project-description').value,
      repositoryPath: document.getElementById('project-repository').value,
    }) });
    state.projectId = project.id;
    showError('');
    await refresh();
  } catch (error) { showError(error.message); }
};
document.getElementById('submit-request').onclick = async () => {
  try {
    const projectId = document.getElementById('request-project').value;
    const text = document.getElementById('request-text').value;
    await api('/api/projects/' + projectId + '/requests', { method: 'POST', body: JSON.stringify({ text }) });
    showError('');
    await refresh();
  } catch (error) { showError(error.message); }
};
for (const id of ['message-filter', 'decision-filter']) {
  document.getElementById(id).addEventListener('input', () => {
    if (!state.projectId) return;
    if (id === 'message-filter') void renderMessages(state.projectId);
    else void renderDecisionHistory(state.projectId);
  });
}

const stream = new EventSource('/events' + (token ? '?token=' + encodeURIComponent(token) : ''));
stream.onopen = () => { document.getElementById('stream').textContent = 'stream connected'; };
stream.onerror = () => { document.getElementById('stream').textContent = 'stream reconnecting'; };
stream.onmessage = () => { void refresh(); };
refresh().catch(error => showError(error.message));
</script>
</body>
</html>`
