/**
 * The NC-AG application entry point: it starts the orchestrator with the model
 * port the deployment configured and serves the control plane.
 *
 * The model port comes from an embedding host that mounted the harness LLM
 * service. When no port exists the platform still starts; every task that needs
 * a model reports the missing route instead of pretending to work.
 * @module @deepseek-ai/dsh-ncag-dashboard/platform
 */

import { NcagOrchestrator, configFromEnvironment, DASHBOARD_HOST_ENV, DASHBOARD_TOKEN_ENV, PORT_ENV } from '@deepseek-ai/dsh-ncag-orchestrator'
import type { OrchestratorConfig, RouteRegistry } from '@deepseek-ai/dsh-ncag-orchestrator'
import type { LlmPort } from '@deepseek-ai/dsh-ncag-runtime'
import { startDashboard } from './index.ts'

/** A running platform process. */
export interface RunningPlatform {
  readonly orchestrator: NcagOrchestrator
  /** URL of the control plane. */
  readonly url: string
  /** Stop the control plane and the orchestrator. */
  stop(): Promise<void>
}

/**
 * Start the platform and serve its control plane.
 * @param environment - process environment.
 * @param llm - model port supplied by an embedding host, or null.
 * @param routeRegistry - provider registry supplied by an embedding host, or null.
 * @returns the running platform.
 */
export async function runPlatform(
  environment: Readonly<Record<string, string | undefined>>,
  llm: LlmPort | null,
  routeRegistry: RouteRegistry | null,
): Promise<RunningPlatform> {
  const config: OrchestratorConfig = configFromEnvironment(environment)
  const orchestrator = new NcagOrchestrator({ config, llm, routeRegistry })
  await orchestrator.start()
  const dashboard = await startDashboard(orchestrator, {
    port: Number(environment[PORT_ENV] ?? 0),
    host: environment[DASHBOARD_HOST_ENV] ?? '127.0.0.1',
    token: environment[DASHBOARD_TOKEN_ENV] ?? null,
  })
  return {
    orchestrator,
    url: dashboard.url,
    stop: async () => {
      await dashboard.stop()
      await orchestrator.stop()
    },
  }
}
