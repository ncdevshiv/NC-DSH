// @vitest-environment jsdom
/** Catalog reads preserve draft ownership and discard responses for a previous provider. */
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { ModelListEditor } from '../src/client/ModelListEditor.tsx'
import type { ModelDiscoveryOutcome, ModelsOperations } from '../src/client/operations.ts'
import { en } from '../src/client/locales.ts'

afterEach(cleanup)

function operations(discoverModels: ModelsOperations['discoverModels']): ModelsOperations {
  return {
    discoverModels,
    testModel: vi.fn(),
    storeCredential: vi.fn(),
    removeCredential: vi.fn(),
    writeSettings: vi.fn(),
  }
}

it('states the capabilities a catalog discloses on the collapsed row', async () => {
  render(<ModelListEditor
    models={[{ id: 'thinker' }, { id: 'plain' }]} onChange={() => {}} catalogProvider="openai"
    probe={{ settingsNs: 'llm-pi-ai', provider: 'openai' }} disabled={false} t={key => en[key]} onBusyChange={() => {}}
    operations={operations(() => Promise.resolve({
      kind: 'found',
      models: [
        { id: 'thinker', reasoning: true, inputModalities: ['text', 'image'], contextWindow: 131_072 },
        { id: 'plain', reasoning: false, inputModalities: ['text'] },
      ],
    }))}
  />)

  // Each fact is a chip on the row itself, so a collapsed list still says what
  // the endpoint disclosed instead of hiding it behind the disclosure.
  await screen.findByText(en.modelThinking)
  expect(screen.getAllByText(en.modelThinking)).toHaveLength(1)
  expect(screen.getAllByText(en.modelVision)).toHaveLength(1)
  expect(screen.getByText(en.modelContextChip.replace('{size}', () => '131072'))).toBeTruthy()
  // The non-reasoning, text-only model states neither fact.
  expect(screen.getAllByText(en.modelVision)).toHaveLength(1)
})

it('names a hand-added row as custom and states the facts the row declares', () => {
  render(<ModelListEditor
    models={[
      { id: 'mine', source: 'custom', input: ['text', 'image'], contextWindow: 128_000 },
      { id: 'adopted', source: 'discovered' },
      { id: 'non-reasoning', reasoningEfforts: false },
      { id: 'reasoning', reasoningEfforts: { off: null, high: 'high' } },
    ]} onChange={() => {}}
    probe={{ settingsNs: 'llm-pi-ai' }} disabled={false} t={key => en[key]} onBusyChange={() => {}}
    operations={operations(() => Promise.resolve({ kind: 'found', models: [] }))}
  />)
  // Only the row a person added states where it came from; an adopted row is
  // ordinary listing output and needs no marker.
  expect(screen.getAllByText(en.customTag)).toHaveLength(1)
  expect(screen.getAllByText(en.modelVision)).toHaveLength(1)
  expect(screen.getByText(en.modelContextChip.replace('{size}', () => '128K'))).toBeTruthy()
  // Declared reasoning levels state thinking support; an explicit `false`
  // states the model cannot think, which is no chip rather than a negative one.
  expect(screen.getAllByText(en.modelThinking)).toHaveLength(1)
})

it('reports a fetch that only pruned stale rows', async () => {
  const onChange = vi.fn()
  const actions = operations(() => Promise.resolve({ kind: 'found', models: [{ id: 'kept' }] }))
  render(<ModelListEditor
    models={[{ id: 'stale', source: 'discovered' }, { id: 'kept', source: 'discovered' }]}
    onChange={onChange} catalogProvider="openai"
    probe={{ settingsNs: 'llm-pi-ai', provider: 'openai' }} disabled={false} t={key => en[key]} onBusyChange={() => {}}
    operations={actions}
  />)
  fireEvent.click(await screen.findByRole('button', { name: en.fetchModels }))
  await screen.findByText(en.fetchRemoved.replace('{removed}', () => '1'))
  expect(onChange).toHaveBeenCalledWith([{ id: 'kept', source: 'discovered' }])
})

it('keeps the list and reports an empty sync instead of pruning the route away', async () => {
  const onChange = vi.fn()
  // The endpoint still reports the catalog row, but the only owned row was
  // fetched earlier and is gone from the reply — syncing would leave nothing.
  const actions = operations(() => Promise.resolve({ kind: 'found', models: [{ id: 'catalog' }] }))
  render(<ModelListEditor
    models={[{ id: 'stale', source: 'discovered' }]} onChange={onChange} catalogProvider="openai"
    probe={{ settingsNs: 'llm-pi-ai', provider: 'openai' }} disabled={false} t={key => en[key]} onBusyChange={() => {}}
    operations={actions}
  />)
  fireEvent.click(await screen.findByRole('button', { name: en.fetchModels }))
  await screen.findByText(en.fetchEmpty)
  // A route that serves no model cannot be resolved, so nothing is written.
  expect(onChange).not.toHaveBeenCalled()
})

it('ignores a late catalog response after the provider changes', async () => {
  const oldCatalog = Promise.withResolvers<ModelDiscoveryOutcome>()
  const newCatalog = Promise.withResolvers<ModelDiscoveryOutcome>()
  const actions = operations(vi.fn()
    .mockReturnValueOnce(oldCatalog.promise)
    .mockReturnValueOnce(newCatalog.promise))
  const onChange = vi.fn()
  const props = {
    models: [{ id: 'm' }], onChange, operations: actions,
    disabled: false, t: (key: keyof typeof en) => en[key], onBusyChange: () => {},
  }
  const { rerender } = render(<ModelListEditor {...props} catalogProvider="old" probe={{ settingsNs: 'llm-pi-ai', provider: 'old' }} />)
  fireEvent.click(screen.getByRole('button', { name: `${en.modelAdvanced} 1` }))
  expect(screen.getByRole<HTMLInputElement>('checkbox', { name: en.modelInputImage }).disabled).toBe(true)
  rerender(<ModelListEditor {...props} catalogProvider="new" probe={{ settingsNs: 'llm-pi-ai', provider: 'new' }} />)
  await act(async () => { newCatalog.resolve({ kind: 'found', models: [{ id: 'm', inputModalities: ['text', 'image'] }] }) })
  expect(screen.getByRole<HTMLInputElement>('checkbox', { name: en.modelInputImage }).checked).toBe(true)
  await act(async () => { oldCatalog.resolve({ kind: 'found', models: [{ id: 'm', inputModalities: ['text'] }] }) })
  expect(screen.getByRole<HTMLInputElement>('checkbox', { name: en.modelInputImage }).checked).toBe(true)
  expect(onChange).not.toHaveBeenCalled()
})

it('uses provider input defaults for a model absent from the installed catalog', async () => {
  const onChange = vi.fn()
  render(<ModelListEditor
    models={[{ id: 'custom' }]} onChange={onChange} defaultInput={['image']} catalogProvider="openai"
    probe={{ settingsNs: 'llm-pi-ai', provider: 'openai' }} disabled={false} t={key => en[key]} onBusyChange={() => {}}
    operations={operations(() => Promise.resolve({ kind: 'found', models: [] }))}
  />)
  fireEvent.click(screen.getByRole('button', { name: `${en.modelAdvanced} 1` }))
  const text = screen.getByRole<HTMLInputElement>('checkbox', { name: en.modelInputText })
  await waitFor(() => { expect(text.disabled).toBe(false) })
  expect(text.checked).toBe(false)
  fireEvent.click(text)
  expect(onChange).toHaveBeenCalledWith([{ id: 'custom', input: ['text', 'image'] }])
})

it('inherits catalog inputs once an incomplete draft has a model id', async () => {
  const onChange = vi.fn()
  const props = {
    onChange, catalogProvider: 'openai',
    probe: { settingsNs: 'llm-pi-ai', provider: 'openai' },
    disabled: false, t: (key: keyof typeof en) => en[key], onBusyChange: () => {},
    operations: operations(() => Promise.resolve({
      kind: 'found', models: [{ id: 'vision', inputModalities: ['text', 'image'] }],
    })),
  }
  const { rerender } = render(<ModelListEditor {...props} models={[{}]} />)
  fireEvent.click(screen.getByRole('button', { name: `${en.modelAdvanced} 1` }))
  const image = screen.getByRole<HTMLInputElement>('checkbox', { name: en.modelInputImage })
  await waitFor(() => { expect(image.disabled).toBe(false) })
  expect(screen.getByRole<HTMLInputElement>('checkbox', { name: en.modelInputText }).checked).toBe(true)
  expect(image.checked).toBe(false)
  expect(onChange).not.toHaveBeenCalled()

  const id = screen.getByLabelText<HTMLInputElement>(`${en.modelId} 1`)
  expect(id.value).toBe('')
  fireEvent.change(id, { target: { value: 'vision' } })
  expect(onChange).toHaveBeenCalledExactlyOnceWith([{ id: 'vision' }])
  rerender(<ModelListEditor {...props} models={[{ id: 'vision' }]} />)
  expect(image.checked).toBe(true)
  expect(onChange).toHaveBeenCalledTimes(1)
})

it('toggles a model and asks the owning card to probe it', () => {
  const onChange = vi.fn()
  const onTestModel = vi.fn()
  const props = {
    models: [{ id: 'm' }], onChange, catalogProvider: 'openai',
    probe: { settingsNs: 'llm-pi-ai', provider: 'openai' }, disabled: false,
    t: (key: keyof typeof en) => en[key], onBusyChange: () => {},
    operations: operations(() => Promise.resolve({ kind: 'found', models: [] })),
    onTestModel,
  }
  const { rerender } = render(<ModelListEditor {...props} />)
  fireEvent.click(screen.getByRole('switch', { name: `${en.modelEnabled} 1` }))
  expect(onChange).toHaveBeenCalledWith([{ id: 'm', enabled: false }])
  fireEvent.click(screen.getByRole('button', { name: `${en.testConnection} 1` }))
  expect(onTestModel).toHaveBeenCalledWith('m')
  rerender(<ModelListEditor {...props} testing={new Set(['m'])} />)
  expect(screen.getByRole('status').textContent).toBe(en.testingConnection)
})

it('restores inherited image input after a failed catalog read is retried manually', async () => {
  const discover = vi.fn<ModelsOperations['discoverModels']>()
    .mockResolvedValueOnce({ kind: 'refused', message: 'Catalog unavailable' })
    .mockResolvedValueOnce({ kind: 'found', models: [{ id: 'vision', inputModalities: ['text', 'image'] }] })
  const onChange = vi.fn()
  render(<ModelListEditor
    models={[{ id: 'vision' }]} onChange={onChange} catalogProvider="openai"
    probe={{ settingsNs: 'llm-pi-ai', provider: 'openai' }} disabled={false} t={key => en[key]} onBusyChange={() => {}}
    operations={operations(discover)}
  />)
  await screen.findByText('Catalog unavailable')
  fireEvent.click(screen.getByRole('button', { name: `${en.modelAdvanced} 1` }))
  const image = screen.getByRole<HTMLInputElement>('checkbox', { name: en.modelInputImage })
  expect(image.checked).toBe(false)

  fireEvent.click(screen.getByRole('button', { name: en.fetchModels }))
  // The reply names the row the list already serves, so the fetch adopts
  // nothing and the inherited types arrive through the catalog instead.
  await screen.findByText(en.fetchCompleteList)
  expect(image.checked).toBe(true)
  expect(screen.queryByText('Catalog unavailable')).toBeNull()
  expect(discover).toHaveBeenCalledTimes(2)
  expect(onChange).not.toHaveBeenCalled()
})
