/**
 * That a browser provider's configuration is actually configurable.
 *
 * The settings surface only reaches a plugin whose Config projects a form. This
 * asserts it against the shipped schema rather than a stand-in, because what
 * makes the browser choice unusual is its shape: a union of launch and attach,
 * whose branches cannot carry volatile fields individually, so the whole
 * document is the editable reference.
 */

import Schema from '@deepseek-ai/schemastery'
import { describe, expect, it } from 'vitest'
import { isVolatilePath, projectForm, volatileForm } from '../../../settings/settings/src/schema.ts'
import { BrowserMcpConfig } from '../src/mcp.ts'

/** The envelope the Host sends: serialized, then rehydrated as the client does. */
function wire(): Schema {
  return new Schema(JSON.parse(JSON.stringify(BrowserMcpConfig.toJSON())) as Schema)
}

/** The form the Host serves, or a failure when it serves none. */
function form(): Schema {
  const served = volatileForm(BrowserMcpConfig)
  if (served === undefined) throw new Error('the browser provider serves no settings form')
  return served
}

/** Every field path the served form addresses, across both browser modes. */
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

describe('browser provider settings form', () => {
  it('serves a form even though its config is a union', () => {
    expect(volatileForm(BrowserMcpConfig)).toBeDefined()
    expect(form().type).toBe('union')
  })

  it('round-trips through the envelope the Host sends', () => {
    expect(wire().type).toBe('union')
    expect(wire().list).toHaveLength(2)
  })

  it('exposes every field a user changes to choose a browser', () => {
    expect(editableFields()).toEqual([
      'endpoint',
      'executablePath',
      'headless',
      'mode',
      'toolCallTimeoutMs',
    ])
  })

  it('accepts an edit to any field of either browser mode', () => {
    for (const path of ['mode', 'headless', 'executablePath', 'endpoint', 'toolCallTimeoutMs']) {
      expect(isVolatilePath(BrowserMcpConfig, path.split('.'))).toBe(true)
    }
  })

  it('projects the declared fields of a launched browser', () => {
    const resolved = (BrowserMcpConfig({ mode: 'launch' }) as unknown as { get(): unknown }).get()
    expect(projectForm(form(), resolved)).toEqual({ mode: 'launch', headless: true })
  })

  it('projects the declared fields of an attached browser', () => {
    const resolved = (BrowserMcpConfig({
      mode: 'attach',
      endpoint: 'wss://browser.example/devtools/browser/id',
    }) as unknown as { get(): unknown }).get()
    expect(projectForm(form(), resolved)).toEqual({
      mode: 'attach',
      endpoint: 'wss://browser.example/devtools/browser/id',
    })
  })
})
