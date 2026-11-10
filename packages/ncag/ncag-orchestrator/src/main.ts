/**
 * The standalone NC-AG runtime: an orchestrator, its HTTP control plane and
 * graceful shutdown, in one process.
 *
 * A deployment configures a database path, a workspace root and model prices;
 * everything else is discovered. The command refuses to start when the
 * required configuration is missing, so a half-configured platform never
 * reports itself healthy.
 * @module @deepseek-ai/dsh-ncag-orchestrator/main
 */

import { NcagOrchestrator } from './orchestrator.ts'
import type { OrchestratorConfig } from './orchestrator.ts'
import { createModelPrices } from './pricing.ts'

/** Environment variable holding the platform database path. */
export const DATABASE_ENV = 'NCAG_DATABASE_PATH'

/** Environment variable holding the agent workspace root. */
export const WORKSPACE_ENV = 'NCAG_WORKSPACE_ROOT'

/** Environment variable holding the JSON model price table. */
export const PRICING_ENV = 'NCAG_MODEL_PRICING'

/** Environment variable holding the preferred provider list. */
export const PROVIDERS_ENV = 'NCAG_PREFERRED_PROVIDERS'

/** Environment variable holding the control-plane port. */
export const PORT_ENV = 'NCAG_PORT'

/** Environment variable holding the control-plane bind address. */
export const DASHBOARD_HOST_ENV = 'NCAG_HOST'

/** Environment variable holding the control-plane API token. */
export const DASHBOARD_TOKEN_ENV = 'NCAG_TOKEN'

/**
 * Resolve configuration from the environment, refusing to invent required values.
 * @param environment - process environment.
 * @returns the validated orchestrator configuration.
 */
export function configFromEnvironment(environment: Readonly<Record<string, string | undefined>>): OrchestratorConfig {
  const databasePath = environment[DATABASE_ENV]
  if (databasePath === undefined || databasePath.trim().length === 0) {
    throw new Error(`ncag: ${DATABASE_ENV} must name the platform database file`)
  }
  const workspaceRoot = environment[WORKSPACE_ENV]
  if (workspaceRoot === undefined || workspaceRoot.trim().length === 0) {
    throw new Error(`ncag: ${WORKSPACE_ENV} must name the directory that holds agent workspaces`)
  }
  const pricing = environment[PRICING_ENV]
  if (pricing === undefined || pricing.trim().length === 0) {
    throw new Error(`ncag: ${PRICING_ENV} must hold the model price table so cost accounting cannot report zero`)
  }
  const prices = createModelPrices(pricing)
  if (prices.length === 0) {
    throw new Error(`ncag: ${PRICING_ENV} must hold the model price table so cost accounting cannot report zero`)
  }
  const providers = environment[PROVIDERS_ENV] ?? ''
  return {
    databasePath,
    workspaceRoot,
    modelPricing: prices,
    preferredProviders: providers.split(',').map(item => item.trim()).filter(item => item.length > 0),
    leaseTtlMs: Number(environment.NCAG_LEASE_TTL_MS ?? 900_000),
    maxConcurrentAgents: Number(environment.NCAG_MAX_CONCURRENT_AGENTS ?? 4),
    maxStepsPerTask: Number(environment.NCAG_MAX_STEPS ?? 24),
    maxOutputTokens: Number(environment.NCAG_MAX_OUTPUT_TOKENS ?? 4096),
    healthIntervalMs: Number(environment.NCAG_HEALTH_INTERVAL_MS ?? 15_000),
    stallTimeoutMs: Number(environment.NCAG_STALL_TIMEOUT_MS ?? 120_000),
    repeatedToolThreshold: Number(environment.NCAG_REPEATED_TOOL_THRESHOLD ?? 4),
  }
}

/**
 * Start the platform and keep it running until the process is asked to stop.
 * @param environment - process environment.
 * @param connect - callback invoked with the started orchestrator.
 * @returns the orchestrator, for embedding hosts that manage their own lifetime.
 */
export async function main(
  environment: Readonly<Record<string, string | undefined>>,
  connect: (orchestrator: NcagOrchestrator, port: number) => void,
): Promise<NcagOrchestrator> {
  const config = configFromEnvironment(environment)
  const port = Number(environment[PORT_ENV] ?? 0)
  const orchestrator = new NcagOrchestrator({ config, llm: null, routeRegistry: null })
  await orchestrator.start()
  connect(orchestrator, port)
  return orchestrator
}
