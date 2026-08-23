// Daemon HTTP helpers. Every call authenticates with the bearer token the
// server injected into index.html as window.__DW_TOKEN__ (placeholder
// "__DATAWORM_TOKEN__" replaced at serve time — same mechanism as legacy).

declare global {
  interface Window {
    __DW_TOKEN__?: string;
  }
}

export const TOKEN: string = window.__DW_TOKEN__ ?? "";

function qs(params: Record<string, string | number | undefined>): string {
  const sp = new URLSearchParams();
  sp.set("token", TOKEN); // EventSource-compatible auth channel
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== null && `${v}`.length) sp.set(k, String(v));
  }
  return sp.toString();
}

/** GET /api/<method>?<params> — throws on transport or {"error": ...} results. */
export async function api<T>(
  method: string,
  params: Record<string, string | number | undefined> = {},
): Promise<T> {
  const url = `/api/${encodeURIComponent(method)}?${qs(params)}`;
  const res = await fetch(url, {
    headers: TOKEN ? { Authorization: `Bearer ${TOKEN}` } : {},
  });
  if (!res.ok) throw new Error(`${method}: HTTP ${res.status}`);
  const body = await res.json();
  if (body && typeof body === "object" && "error" in body && body.error) {
    throw new Error(String(body.error));
  }
  return body as T;
}

/** POST /rpc — JSON-RPC envelope; unwraps result or throws on error. */
export async function rpc<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  const res = await fetch("/rpc", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...(TOKEN ? { Authorization: `Bearer ${TOKEN}` } : {}),
    },
    body: JSON.stringify({ jsonrpc: "2.0", method, params, id: 1 }),
  });
  if (!res.ok) throw new Error(`rpc ${method}: HTTP ${res.status}`);
  const body = await res.json();
  if (body.error) throw new Error(body.error.message ?? String(body.error));
  return body.result as T;
}
