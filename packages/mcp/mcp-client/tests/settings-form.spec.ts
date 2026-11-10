/**
 * That this server's configuration is actually configurable.
 *
 * The settings surface only reaches a plugin whose Config projects a form, and
 * that projection decides whether a server is editable from the UI at all.
 * These assert it against the shipped schema rather than a stand-in, because
 * what makes this server unusual is its shape: a union of two transports, whose
 * branches cannot carry volatile fields individually, so the whole document is
 * the editable reference.
 */

import Schema from '@deepseek-ai/schemastery'
import type { Volatile } from '@deepseek-ai/cordis'
import { describe, expect, it } from 'vitest'
import { Config } from '../src/index.ts'
import { isVolatilePath, projectForm, volatileForm } from '../../../settings/settings/src/schema.ts'

/** The envelope the Host sends: serialized, then rehydrated as the client does. */
function wire(): Schema {
  return new Schema(JSON.parse(JSON.stringify(Config.toJSON())) as Schema)
}

/** The form the Host serves, or a failure when it serves none. */
function form(): Schema {
  const served = volatileForm(Config)
  if (served === undefined) throw new Error('mcp-client serves no settings form')
  return served
}

/** Every field path the served form addresses, across both transports. */
function editableFields(): string[] {
  const paths: string[] = []
  const walk = (node: Schema, path: string[]): void => {
    if (node.type === 'union') {
      for (const branch of node.list ?? []) walk(branch, path)
      return
    }
    if (node.type !== 'object') {
      paths.push(path.join('.'))
      return
    }
    for (const [key, child] of Object.entries(node.dict ?? {})) walk(child as Schema, [...path, key])
  }
  walk(form(), [])
  return [...new Set(paths)].sort()
}

describe('mcp-client settings form', () => {
  it('serves a form even though its config is a union', () => {
    // A union cannot carry volatile fields on its branches, so the whole
    // document is the reference. Without it this namespace is never served.
    expect(volatileForm(Config)).toBeDefined()
    expect(form().type).toBe('union')
  })

  it('round-trips through the envelope the Host sends', () => {
    // Both branches share one `reconnect` node, so `toJSON` emits a `refs`
    // table; rehydrating is what resolves it back to a two-branch union.
    expect(wire().type).toBe('union')
    expect(wire().list).toHaveLength(2)
  })

  it('exposes every field a user changes to configure a server', () => {
    expect(editableFields()).toEqual([
      'args',
      'command',
      'cwd',
      'env',
      'failOnStartupError',
      'headers',
      'maxInstructionBytes',
      'reconnect.enabled',
      'reconnect.initialDelayMs',
      'reconnect.maxAttempts',
      'reconnect.maxDelayMs',
      'serverName',
      'toolCallTimeoutMs',
      'transport',
      'url',
    ])
  })

  it('accepts an edit to any field of either transport', () => {
    for (const path of ['transport', 'serverName', 'command', 'args', 'env', 'cwd', 'url', 'headers',
      'toolCallTimeoutMs', 'maxInstructionBytes', 'reconnect.initialDelayMs']) {
      expect(isVolatilePath(Config, path.split('.'))).toBe(true)
    }
  })

  it('projects the declared fields of a stdio server', () => {
    // The Host resolves defaults first, then projects the resolved value.
    const resolved = (Config({ transport: 'stdio', serverName: 'srv', command: 'echo' }) as unknown as Volatile<never>).get()
    expect(projectForm(form(), resolved)).toEqual({
      transport: 'stdio',
      serverName: 'srv',
      command: 'echo',
      args: [],
      env: {},
      cwd: '',
      toolCallTimeoutMs: 60_000,
      failOnStartupError: false,
      maxInstructionBytes: 32_768,
      reconnect: { enabled: true, initialDelayMs: 500, maxDelayMs: 30_000, maxAttempts: 10 },
    })
  })

  it('projects the declared fields of a streamable-http server', () => {
    const resolved = (Config({
      transport: 'streamable-http', serverName: 'web', url: 'https://example.test/mcp',
    }) as unknown as Volatile<never>).get()
    const projected = projectForm(form(), resolved) as Record<string, unknown>
    expect(projected).toMatchObject({
      transport: 'streamable-http',
      serverName: 'web',
      url: 'https://example.test/mcp',
      headers: {},
      toolCallTimeoutMs: 60_000,
      failOnStartupError: false,
    })
    // A stdio-only field has no place in an http server's section.
    expect(projected).not.toHaveProperty('command')
  })
})
