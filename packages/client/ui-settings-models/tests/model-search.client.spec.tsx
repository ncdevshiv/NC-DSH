// @vitest-environment jsdom
/** The model search field narrows a provider's model list while the user types. */
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { DeepSeekModelsEditor } from '../src/client/DeepSeekModelsEditor.tsx'
import { ModelListEditor } from '../src/client/ModelListEditor.tsx'
import { modelMatches } from '../src/client/model-search.ts'
import { en } from '../src/client/locales.ts'
import type { ModelsOperations } from '../src/client/operations.ts'

afterEach(cleanup)

const t = (key: keyof typeof en): string => en[key]

function operations(): ModelsOperations {
  return {
    discoverModels: vi.fn<ModelsOperations['discoverModels']>(() => Promise.resolve({ kind: 'found', models: [] })),
    testModel: vi.fn(),
    storeCredential: vi.fn(),
    removeCredential: vi.fn(),
    writeSettings: vi.fn(),
  }
}

it('matches a model by id or display name, ignoring case and separators', () => {
  expect(modelMatches({ id: 'gpt-4o' }, '')).toBe(true)
  expect(modelMatches({ id: 'gpt-4o' }, '  ')).toBe(true)
  expect(modelMatches({ id: 'gpt-4o' }, 'GPT')).toBe(true)
  expect(modelMatches({ id: 'gpt-4o-mini' }, '4o mini')).toBe(true)
  expect(modelMatches({ id: 'deepseek-v4-flash' }, 'flash v4')).toBe(true)
  expect(modelMatches({ id: 'gpt-4o', name: 'Turbo' }, 'turbo')).toBe(true)
  expect(modelMatches({ id: 'gpt-4o' }, 'zzz')).toBe(false)
})

it('matches a term whose characters appear in order inside one word', () => {
  expect(modelMatches({ id: 'gpt-4o' }, 'gpt4o')).toBe(true)
  expect(modelMatches({ id: 'gemini-4-argon' }, 'gmn')).toBe(true)
  expect(modelMatches({ id: 'gemini-4-argon' }, 'gmz')).toBe(false)
  expect(modelMatches({ id: 'gemini-4-argon' }, 'gm')).toBe(false)
  // The loose match stays inside one word, so a term cannot name a model by
  // characters spread across its parts.
  expect(modelMatches({ id: 'ogg/beam-free' }, 'gem')).toBe(false)
})

it('filters the served model rows as the term is typed', () => {
  render(<ModelListEditor
    models={[{ id: 'ogg/beam-free' }, { id: 'ogg/deepseek-v4.1-flash' }, { id: 'ogg/gemini-4-argon' }]}
    onChange={vi.fn()}
    probe={{ settingsNs: 'llm-pi-ai' }}
    operations={operations()}
    t={t}
    disabled={false}
    onBusyChange={() => {}}
  />)

  const search = screen.getByLabelText(en.searchModels)
  fireEvent.change(search, { target: { value: 'gem' } })
  expect(screen.queryByLabelText(`${en.modelId} 1`)).toBeNull()
  expect(screen.queryByLabelText(`${en.modelId} 2`)).toBeNull()
  // A hidden row keeps its place in the served list, so the row it names is
  // still the row its number addresses.
  expect(screen.getByLabelText(`${en.modelId} 3`)).toBeTruthy()

  fireEvent.change(search, { target: { value: 'gem zzz' } })
  expect(screen.queryByLabelText(`${en.modelId} 3`)).toBeNull()
  expect(screen.getByText(en.noMatchingModels)).toBeTruthy()

  fireEvent.change(search, { target: { value: '' } })
  expect(screen.queryByText(en.noMatchingModels)).toBeNull()
  expect(screen.getAllByLabelText(new RegExp(`^${en.modelId} \\d+$`))).toHaveLength(3)
})

it('filters the deepseek catalog rows by display name', () => {
  render(<DeepSeekModelsEditor
    models={[{ id: 'deepseek-chat', name: 'DeepSeek Chat' }, { id: 'deepseek-reasoner', name: 'DeepSeek Reasoner' }]}
    overridden={false}
    defaultContextWindow={undefined}
    defaultMaxTokens={undefined}
    t={t}
    disabled={false}
    onChange={vi.fn()}
    onReset={vi.fn()}
  />)

  const search = screen.getByLabelText(en.searchModels)
  fireEvent.change(search, { target: { value: 'reasoner' } })
  expect(screen.queryByLabelText(`${en.modelId} 1`)).toBeNull()
  expect(screen.getByLabelText(`${en.modelId} 2`)).toBeTruthy()
})
