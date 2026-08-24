import { describe, expect, it } from 'vitest'
import { createStopBlockLedger, DEFAULT_MAX_CONSECUTIVE_STOP_BLOCKS } from '../src/stop-guard.ts'

/** The ledger semantics both hook bridges rely on for their Stop-loop cap. */

describe('createStopBlockLedger', () => {
  it('reports zero blocks before anything is recorded', () => {
    const ledger = createStopBlockLedger()
    const agent = {}
    expect(ledger.blocks(agent, 1)).toBe(0)
  })

  it('accumulates recorded blocks within one turn', () => {
    const ledger = createStopBlockLedger()
    const agent = {}
    ledger.recordBlock(agent, 3)
    ledger.recordBlock(agent, 3)
    expect(ledger.blocks(agent, 3)).toBe(2)
  })

  it('starts fresh when the agent opens its next turn', () => {
    const ledger = createStopBlockLedger()
    const agent = {}
    ledger.recordBlock(agent, 1)
    ledger.recordBlock(agent, 1)
    expect(ledger.blocks(agent, 2)).toBe(0)
    // Recording the new turn replaces the stored chain; only the open turn's
    // budget is ever consulted at a stopping boundary.
    ledger.recordBlock(agent, 2)
    expect(ledger.blocks(agent, 1)).toBe(0)
    expect(ledger.blocks(agent, 2)).toBe(1)
  })

  it('keeps agents independent', () => {
    const ledger = createStopBlockLedger()
    const parent = {}
    const child = {}
    ledger.recordBlock(parent, 1)
    expect(ledger.blocks(child, 1)).toBe(0)
  })

  it('counts a block recorded after a read of an unrecorded turn', () => {
    const ledger = createStopBlockLedger()
    const agent = {}
    expect(ledger.blocks(agent, 7)).toBe(0)
    ledger.recordBlock(agent, 7)
    expect(ledger.blocks(agent, 7)).toBe(1)
  })

  it('pins the shared default cap so catalog changes are deliberate', () => {
    expect(DEFAULT_MAX_CONSECUTIVE_STOP_BLOCKS).toBe(25)
  })
})
