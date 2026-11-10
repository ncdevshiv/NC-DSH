/**
 * NC-AG composed into the harness: the platform service and its model-facing
 * tools. Included by a dsh profile bundle list, or layered directly.
 * @module @deepseek-ai/dsh-ncag
 */

export { apply, inject, name } from '@deepseek-ai/dsh-tool-ncag'
export { createNcagTools } from '@deepseek-ai/dsh-tool-ncag'
export * from '@deepseek-ai/dsh-ncag-orchestrator'
export * from '@deepseek-ai/dsh-ncag-core'
export * from '@deepseek-ai/dsh-ncag-store'
export * from '@deepseek-ai/dsh-ncag-runtime'
export * from '@deepseek-ai/dsh-ncag-dashboard'
