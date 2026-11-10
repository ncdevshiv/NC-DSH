/**
 * A minimal MCP server that speaks the nc-tools tool surface the filesystem
 * provider calls, backed by a real directory.
 *
 * The provider's contract is a MAPPING: an nc-tools result shape becomes a
 * filesystem-seam result shape. A canned-response mock would prove the
 * mapping against responses the provider itself dictated, which is circular.
 * This server therefore does the real work on a real directory — real
 * content, real hashes, real occurrence counts — so the mapping is checked
 * against behavior the provider did not choose.
 *
 * Plain JavaScript on purpose: the provider spawns it with plain `node`, so
 * the test needs no TypeScript loader in the child.
 *
 * Run: node fixture-nc-tools-server.mjs <workspace-root>
 */

import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { McpServer } from '@modelcontextprotocol/server'
import { serveStdio } from '@modelcontextprotocol/server/stdio'
import { z } from 'zod'

const root = resolve(process.argv[2] ?? process.cwd())

/** Resolve a caller-supplied path inside the served root. */
function served(path) {
  const absolute = resolve(root, path)
  if (absolute !== root && !absolute.startsWith(root + '/') && !absolute.startsWith(root + '\\')) {
    throw new Error(`path outside the served root: ${path}`)
  }
  return absolute
}

/** The content digest of a file. */
function digestOf(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex')
}

/** How many times a literal appears in a file's text. */
function occurrences(path, needle) {
  if (needle === '') return 0
  const text = readFileSync(path, 'utf8')
  let count = 0
  let at = text.indexOf(needle)
  while (at >= 0) {
    count += 1
    at = text.indexOf(needle, at + needle.length)
  }
  return count
}

/** Answer one tool call with a JSON payload. */
function json(value) {
  return { content: [{ type: 'text', text: JSON.stringify(value) }] }
}

/**
 * Answer one tool call with a JSON payload AND the structured content a tool
 * with an output schema must return. The protocol requires the structured
 * form when a schema is declared, and a modern server sends it either way.
 */
function structured(value) {
  return { content: [{ type: 'text', text: JSON.stringify(value) }], structuredContent: value }
}

/** Answer one tool call with a structured failure. */
function failure(code, message, hint) {
  return {
    isError: true,
    content: [{
      type: 'text',
      text: JSON.stringify({ error: { code, message, ...(hint === undefined ? {} : { hint }) } }),
    }],
  }
}

/**
 * The failure a read should report, by the target's file name.
 *
 * The provider translates the server's failure into the seam's error
 * vocabulary, and that translation has to survive every shape a server can
 * send: a plain-text failure with no JSON, a structured failure with no
 * code, and a structured failure with a code and a remediation hint.
 */
function shapedFailure(path) {
  const name = path.split(/[\\/]/).pop() ?? ''
  if (name === 'raise-plain.txt') {
    return { isError: true, content: [{ type: 'text', text: 'plain failure, no structure' }] }
  }
  if (name === 'raise-nocode.txt') {
    return {
      isError: true,
      content: [{ type: 'text', text: JSON.stringify({ error: { message: 'no code here' } }) }],
    }
  }
  if (name === 'raise-hinted.txt') {
    return failure('ERR_PERMISSION', 'the path is not writable', { path, hint: 'ask for access' })
  }
  if (name === 'raise-bare.txt') {
    // The error field is not an object at all.
    return {
      isError: true,
      content: [{ type: 'text', text: JSON.stringify({ error: 'a bare string' }) }],
    }
  }
  if (name === 'raise-nomessage.txt') {
    // The error object carries a code but no message.
    return {
      isError: true,
      content: [{ type: 'text', text: JSON.stringify({ error: { code: 'ERR_IO' } }) }],
    }
  }
  return undefined
}

/**
 * The kind a stat should report, by the target's file name.
 *
 * A directory reports 'dir' and a regular file 'file'. The seam also
 * models a third kind — 'other' — for an entry that is neither, so the
 * provider has to carry it through rather than collapse it.
 */
function statType(path, info) {
  const name = path.split(/[\\/]/).pop() ?? ''
  if (name === 'odd-type.txt') return 'other'
  return info.isDirectory() ? 'dir' : 'file'
}

/**
 * How many times a stat for one path has been answered.
 *
 * The watcher reads the target once to establish its baseline and then
 * polls. A file that becomes unreadable AFTER the baseline is what a
 * watcher has to survive, so the fixture fails the second and later reads
 * of one path.
 */
const statCalls = new Map()

const server = new McpServer(
  { name: 'nc-tools-fixture', version: '1.0.0' },
  { capabilities: { tools: { listChanged: true } } },
)

server.registerTool('fs.stat', {
  description: 'Presence, kind, size and mtime of a path.',
  inputSchema: z.object({ path: z.string() }),
  // The output schema makes the server send STRUCTURED content alongside the
  // text block, which is what a modern MCP server does and what the
  // provider prefers when it is present.
  outputSchema: z.object({
    exists: z.boolean(),
    path: z.string(),
    type: z.string().nullable().optional(),
    size: z.number().nullable().optional(),
    mtimeMs: z.number().nullable().optional(),
  }),
}, async ({ path }) => {
  const absolute = served(path)
  const seen = (statCalls.get(path) ?? 0) + 1
  statCalls.set(path, seen)
  if ((path.split(/[\\/]/).pop() ?? '') === 'raise-stat.txt' && seen > 1) {
    return failure('ERR_IO', 'the stat failed')
  }
  if (!existsSync(absolute)) return structured({ exists: false, path })
  const info = statSync(absolute)
  // A directory reports no size and no mtime: the tool schema declares both
  // nullable, so a conforming server may omit them, and the provider has to
  // cope with that.
  if (info.isDirectory()) {
    return structured({ exists: true, path, type: 'dir', size: null, mtimeMs: null })
  }
  return structured({
    exists: true,
    path,
    type: statType(path, info),
    size: info.size,
    mtimeMs: info.mtimeMs,
  })
})

server.registerTool('fs.hash', {
  description: 'The sha256 digest of a file.',
  inputSchema: z.object({ path: z.string() }),
}, async ({ path }) => {
  const absolute = served(path)
  // A file the server cannot digest: the version token has to fail loudly
  // rather than become a placeholder that lets a stale write through.
  if ((path.split(/[\\/]/).pop() ?? '') === 'no-hash.txt') {
    return json({ algo: 'sha256', sha256: null, bytes: null })
  }
  if (!existsSync(absolute)) return json({ sha256: null, bytes: null })
  return json({
    algo: 'sha256',
    sha256: digestOf(absolute),
    bytes: statSync(absolute).size,
  })
})

server.registerTool('fs.readRange', {
  description: 'A decoded byte window of a file.',
  inputSchema: z.object({
    path: z.string(),
    byteOffset: z.number().optional(),
    maxBytes: z.number().optional(),
  }),
}, async ({ path, byteOffset, maxBytes }) => {
  const absolute = served(path)
  const shaped = shapedFailure(path)
  if (shaped !== undefined) return shaped
  if (!existsSync(absolute)) return failure('ERR_NOT_FOUND', `No such file: ${path}`)
  const bytes = readFileSync(absolute)
  const offset = byteOffset ?? 0
  const cap = maxBytes ?? 65_536
  const window = bytes.subarray(offset, Math.min(bytes.length, offset + cap))
  const content = window.toString('utf8')
  const next = offset + window.length
  return json({
    path,
    fileSize: bytes.length,
    byteOffset: offset,
    byteLength: window.length,
    nextByteOffset: next,
    eof: next >= bytes.length,
    startLine: 1,
    lines: content.split('\n').length,
    content,
  })
})

server.registerTool('fs.readBytes', {
  description: 'A base64 byte window of a file.',
  inputSchema: z.object({
    path: z.string(),
    byteOffset: z.number().optional(),
    maxBytes: z.number().optional(),
  }),
}, async ({ path, byteOffset, maxBytes }) => {
  const absolute = served(path)
  const shaped = shapedFailure(path)
  if (shaped !== undefined) return shaped
  if (!existsSync(absolute)) return failure('ERR_NOT_FOUND', `No such file: ${path}`)
  const bytes = readFileSync(absolute)
  const offset = byteOffset ?? 0
  const cap = maxBytes ?? 65_536
  const window = bytes.subarray(offset, Math.min(bytes.length, offset + cap))
  const next = offset + window.length
  return json({
    path,
    base64: window.toString('base64'),
    byteOffset: offset,
    byteLength: window.length,
    nextByteOffset: next,
    fileSize: bytes.length,
    eof: next >= bytes.length,
  })
})

server.registerTool('fs.list', {
  description: 'The direct children of a directory.',
  inputSchema: z.object({ path: z.string() }),
}, async ({ path }) => {
  const absolute = served(path)
  const entries = readdirSync(absolute, { withFileTypes: true }).map(entry => ({
    name: entry.name,
    path: join(path, entry.name).replace(/\\/g, '/'),
    type: entry.isDirectory() ? 'dir' : 'file',
    size: entry.isFile() ? statSync(join(absolute, entry.name)).size : null,
  }))
  return json({ entries, total: entries.length, truncated: false })
})

server.registerTool('fs.write', {
  description: 'Write a file, creating or replacing it.',
  inputSchema: z.object({ path: z.string(), content: z.string() }),
}, async ({ path, content }) => {
  const absolute = served(path)
  const existed = existsSync(absolute)
  const previousHash = existed ? digestOf(absolute) : null
  mkdirSync(join(absolute, '..'), { recursive: true })
  writeFileSync(absolute, content, 'utf8')
  return json({
    path,
    bytes: Buffer.byteLength(content, 'utf8'),
    created: !existed,
    overwritten: existed,
    previousHash,
  })
})

server.registerTool('patch.check', {
  description: 'What an edit would change, without changing it.',
  inputSchema: z.object({
    path: z.string(),
    edits: z.array(z.object({ oldText: z.string(), newText: z.string() })),
  }),
}, async ({ path, edits }) => {
  const absolute = served(path)
  return json({
    path,
    allValid: true,
    warnings: [],
    edits: edits.map((edit, index) => ({
      index,
      valid: true,
      occurrences: occurrences(absolute, edit.oldText),
    })),
  })
})

server.registerTool('patch.apply', {
  description: 'Apply a literal edit, refusing a count that does not match.',
  inputSchema: z.object({
    path: z.string(),
    edits: z.array(z.object({
      oldText: z.string(),
      newText: z.string(),
      expectedCount: z.number().optional(),
    })),
  }),
}, async ({ path, edits }) => {
  const absolute = served(path)
  const before = readFileSync(absolute, 'utf8')
  const previousHash = digestOf(absolute)
  let text = before
  const applied = []
  for (const [index, edit] of edits.entries()) {
    const found = occurrences(absolute, edit.oldText)
    const expected = edit.expectedCount ?? 1
    if (found !== expected) {
      return {
        isError: true,
        content: [{
          type: 'text',
          text: JSON.stringify({
            error: {
              code: found === 0 ? 'PATCH_NO_MATCH' : 'PATCH_AMBIGUOUS',
              message: `expected ${expected} occurrence(s), found ${found}`,
            },
          }),
        }],
      }
    }
    text = text.split(edit.oldText).join(edit.newText)
    applied.push({ index, replacements: found })
  }
  writeFileSync(absolute, text, 'utf8')
  return json({
    path,
    bytes: Buffer.byteLength(text, 'utf8'),
    previousHash,
    sha256: createHash('sha256').update(text).digest('hex'),
    applied,
    warnings: [],
  })
})

/**
 * The served server. `serveStdio` takes a FACTORY, not an instance: it
 * builds the server per connection, and handing it an already-built server
 * fails the handshake with an internal error.
 */
function createServer() {
  return server
}

await serveStdio(createServer)
