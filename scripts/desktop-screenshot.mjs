#!/usr/bin/env node
/**
 * External client for the dsh-desktop debug-capture endpoint. Discovers live
 * desktop shells through their discovery records under
 * `%TEMP%/dsh-desktop-debug/`, then fetches one window screenshot as PNG.
 *
 * Usage:
 *   node scripts/desktop-screenshot.mjs [out.png] [--pid <pid>] [--window <id>]
 *   node scripts/desktop-screenshot.mjs --list
 *
 * The dsh agent uses this same script through its shell tool; any other local
 * process can speak the two HTTP routes directly (see apps/desktop/README.md).
 */
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { exit } from 'node:process'

const DEBUG_DIR = join(tmpdir(), 'dsh-desktop-debug')

/**
 * Endpoint records whose pid still owns a process. A crashed shell leaves its
 * record behind; liveness here keeps residue from answering.
 */
function discoverEndpoints() {
  let names
  try {
    names = readdirSync(DEBUG_DIR)
  } catch {
    return []
  }
  const endpoints = []
  for (const name of names) {
    const match = /^endpoint-(\d+)\.json$/.exec(name)
    if (match === null) continue
    const pid = Number(match[1])
    try {
      process.kill(pid, 0)
    } catch {
      continue
    }
    endpoints.push({ pid, ...JSON.parse(readFileSync(join(DEBUG_DIR, name), 'utf8')) })
  }
  return endpoints.sort((a, b) => a.pid - b.pid)
}

async function listWindows(endpoint) {
  const response = await fetch(`http://127.0.0.1:${endpoint.port}/debug/windows.json?token=${endpoint.token}`)
  if (!response.ok) throw new Error(`windows.json ${response.status}: ${await response.text()}`)
  const body = await response.json()
  for (const window of body.windows) {
    console.log(`pid=${body.pid} window=${window.id} minimized=${window.minimized} title=${JSON.stringify(window.title)} url=${window.url}`)
  }
}

async function capture(endpoint, windowId, outPath) {
  const suffix = windowId === undefined ? '' : `&window=${windowId}`
  const response = await fetch(
    `http://127.0.0.1:${endpoint.port}/debug/screenshot.png?token=${endpoint.token}${suffix}`,
  )
  if (!response.ok) throw new Error(`screenshot ${response.status}: ${await response.text()}`)
  const bytes = Buffer.from(await response.arrayBuffer())
  writeFileSync(outPath, bytes)
  console.log(`${outPath} (${bytes.length} bytes, pid=${endpoint.pid}${windowId === undefined ? '' : ` window=${windowId}`})`)
}

function usage(message) {
  if (message !== undefined) console.error(`error: ${message}`)
  console.error('usage: node scripts/desktop-screenshot.mjs [out.png] [--pid <pid>] [--window <id>] | --list')
  exit(2)
}

const args = process.argv.slice(2)
const endpoints = discoverEndpoints()
if (endpoints.length === 0) {
  console.error(`no live dsh desktop shell found (no pid-live record in ${DEBUG_DIR}); start it with \`bun run desktop\``)
  exit(1)
}

let listOnly = false
let pid
let windowId
const positional = []
for (let at = 0; at < args.length; at += 1) {
  if (args[at] === '--list') listOnly = true
  else if (args[at] === '--pid') pid = Number(args[at += 1])
  else if (args[at] === '--window') windowId = Number(args[at += 1])
  else positional.push(args[at])
}
if (Number.isNaN(pid) || Number.isNaN(windowId)) usage('--pid/--window need numeric values')

const endpoint = pid === undefined ? endpoints[0] : endpoints.find((entry) => entry.pid === pid)
if (endpoint === undefined) {
  console.error(`no live shell with pid ${pid}; live: ${endpoints.map((entry) => entry.pid).join(', ')}`)
  exit(1)
}
if (pid === undefined && endpoints.length > 1) {
  console.error(`multiple live shells (${endpoints.map((entry) => entry.pid).join(', ')}); pick one with --pid`)
  exit(1)
}

try {
  if (listOnly) await listWindows(endpoint)
  else await capture(endpoint, windowId, resolve(positional[0] ?? `dsh-desktop-shot-pid${endpoint.pid}.png`))
} catch (error) {
  console.error(String(error))
  exit(1)
}
