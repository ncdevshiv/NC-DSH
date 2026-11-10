import { describe, expect, it } from 'vitest'
import {
  NcagStateError, allowedTransitions, canTransitionAgent, canTransitionTask, transitionAgent, transitionTask,
} from '../src/machines.ts'

describe('ncag lifecycle machines', () => {
  it('describes an invalid transition with its machine and endpoints', () => {
    const error = new NcagStateError('task', 'COMPLETED', 'READY')
    expect(error.name).toBe('NcagStateError')
    expect(error.machine).toBe('task')
    expect(error.from).toBe('COMPLETED')
    expect(error.to).toBe('READY')
    expect(error.message).toBe('ncag: task cannot move from COMPLETED to READY')
  })

  it('accepts a modeled agent transition and refuses an unmodeled one', () => {
    expect(canTransitionAgent('CREATED', 'INITIALIZING')).toBe(true)
    expect(canTransitionAgent('CREATED', 'WORKING')).toBe(false)
    expect(transitionAgent('CREATED', 'INITIALIZING')).toBe('INITIALIZING')
    expect(transitionAgent('WORKING', 'WORKING')).toBe('WORKING')
    expect(() => transitionAgent('STOPPED', 'WORKING')).toThrow(NcagStateError)
  })

  it('accepts a modeled task transition and refuses an unmodeled one', () => {
    expect(canTransitionTask('PLANNED', 'READY')).toBe(true)
    expect(canTransitionTask('COMPLETED', 'READY')).toBe(false)
    expect(transitionTask('PLANNED', 'READY')).toBe('READY')
    expect(transitionTask('COMPLETED', 'COMPLETED')).toBe('COMPLETED')
    expect(() => transitionTask('COMPLETED', 'READY')).toThrow(NcagStateError)
  })

  it('lists the permitted destinations of both machines', () => {
    expect(allowedTransitions('agent', 'CREATED')).toEqual(['INITIALIZING', 'STOPPING', 'FAILED'])
    expect(allowedTransitions('task', 'PLANNED')).toEqual(['WAITING_APPROVAL', 'READY', 'CANCELLED'])
  })
})
