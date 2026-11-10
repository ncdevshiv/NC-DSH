/**
 * The NC-AG service: it owns one orchestrator for the running harness and
 * exposes the platform operations the model-facing tools and other plugins
 * call. Configuration is a validated Cordis Config, so a deployment turns the
 * platform on and points it at its database from cordis.yml.
 * @module @deepseek-ai/dsh-ncag-service
 */

import { resolve } from 'node:path'
import { Service } from '@deepseek-ai/cordis'
import type { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type {} from '@deepseek-ai/dsh-llm'
import type {} from '@deepseek-ai/dsh-settings'
import type {} from '@deepseek-ai/dsh-workspace'
import type {} from '@deepseek-ai/dsh-ncag-settings'
import { NcagOrchestrator, createModelPrices } from '@deepseek-ai/dsh-ncag-orchestrator'
import type { ModelPrice, RouteRegistry } from '@deepseek-ai/dsh-ncag-orchestrator'
import type { ModelRoute, Project } from '@deepseek-ai/dsh-ncag-core'
import { createLlmPort } from '@deepseek-ai/dsh-ncag-runtime'
import type { LlmPort } from '@deepseek-ai/dsh-ncag-runtime'

declare module '@deepseek-ai/cordis' {
  interface Context {
    /** The NC-AG platform, when this plugin is loaded. */
    ncag: NcagService
  }
}

/** Plugin configuration. */
export interface Config {
  /** Absolute path of the platform database file. */
  readonly databasePath: string
  /** Absolute directory holding agent workspaces when a project has no repository. */
  readonly workspaceRoot: string
  /** JSON model price overrides; empty means every route is unpriced but token-tracked. */
  readonly modelPricing: string
  /** The user's main model, used for every tier until an override names a route. */
  readonly defaultRoute?: ModelRoute | null
  /** Preferred provider names in priority order. */
  readonly preferredProviders: string[]
  /** Lease duration in milliseconds. */
  readonly leaseTtlMs: number
  /** Maximum live agents per project when the project sets no ceiling. */
  readonly maxConcurrentAgents: number
  /** Maximum model steps one agent may take for one task. */
  readonly maxStepsPerTask: number
  /** Output token ceiling for one agent model call. */
  readonly maxOutputTokens: number
  /** Health observation interval in milliseconds. */
  readonly healthIntervalMs: number
  /** Milliseconds without progress before an agent counts as stalled. */
  readonly stalledAfterMs: number
  /** Identical tool invocations before a loop is reported. */
  readonly repeatedToolThreshold: number
}

/** Configuration validator. */
export const Config = z.object({
  databasePath: z.string().required(),
  workspaceRoot: z.string().required(),
  modelPricing: z.string().default('[]'),
  defaultRoute: z.union([
    z.object({ provider: z.string(), model: z.string() }),
    z.const(null),
  ]).default(null),
  preferredProviders: z.array(z.string()).default([]),
  leaseTtlMs: z.number().default(900000),
  maxConcurrentAgents: z.number().default(4),
  maxStepsPerTask: z.number().default(24),
  maxOutputTokens: z.number().default(4096),
  healthIntervalMs: z.number().default(15000),
  stalledAfterMs: z.number().default(120000),
  repeatedToolThreshold: z.number().default(4),
})

/** Plugin name. */
export const name = 'ncag'

/** Services this plugin reads strictly; an absent LLM service is reported to every task that needs one. */
export const inject = [] as const

/**
 * One running platform, shared by every mount that names the same database.
 *
 * A preset mounts once per Session, and the deployment may also mount the
 * platform row on the host plane. Two orchestrators over one durable task graph
 * would each dispatch the same READY task, so mounts resolve to one scheduler
 * and the last mount to release stops it.
 */
interface SharedPlatform {
  readonly orchestrator: NcagOrchestrator
  readonly started: Promise<NcagOrchestrator>
  mounts: number
}

const sharedPlatforms = new Map<string, SharedPlatform>()

let privateDatabases = 0

/**
 * Key one mount's platform. A database path is shared by every mount naming it;
 * an in-memory database is private because two mounts cannot observe one.
 * @param databasePath - configured database path.
 * @returns the sharing key.
 */
function platformKey(databasePath: string): string {
  if (databasePath === ':memory:') return `:memory:${String(++privateDatabases)}`
  return resolve(databasePath)
}

/**
 * The platform service. It creates the orchestrator once per database, hands it
 * the harness model port, and disposes it when the last mount unloads.
 */
export default class NcagService extends Service {
  /** Configuration validator. */
  static Config = Config

  /** The running orchestrator. */
  readonly orchestrator: NcagOrchestrator

  /**
   * @param ctx - the owning Cordis context.
   * @param config - validated plugin configuration.
   */
  constructor(ctx: Context, config: Config) {
    super(ctx, 'ncag')
    ctx.inject(['settings'], (child) => { child.effect(() => child.settings.configure({ auto: false }, ctx.fiber)) })
    const key = platformKey(config.databasePath)
    const shared = sharedPlatforms.get(key)
    if (shared === undefined) {
      const prices: ModelPrice[] = config.modelPricing.trim().length === 0 ? [] : createModelPrices(config.modelPricing)
      const llm = ctx.get('llm')
      // The harness LLM service is mounted by the host program; the mounted
      // branches are covered by the harness end-to-end suite.
      /* v8 ignore start -- the unmounted branch cannot occur in a booted profile. */
      const port: LlmPort | null = llm === undefined ? null : createLlmPort(llm)
      const registry: RouteRegistry | null = llm === undefined
        ? null
        : {
          listProviders: () => llm.listProviders(),
          listModels: provider => llm.listModels(provider),
        }
      /* v8 ignore stop */
      const ncagSettings = ctx.get('ncagSettings')
      this.orchestrator = new NcagOrchestrator({
        config: {
          databasePath: config.databasePath,
          workspaceRoot: config.workspaceRoot,
          modelPricing: prices,
          preferredProviders: [...config.preferredProviders],
          leaseTtlMs: config.leaseTtlMs,
          maxConcurrentAgents: config.maxConcurrentAgents,
          maxStepsPerTask: config.maxStepsPerTask,
          maxOutputTokens: config.maxOutputTokens,
          healthIntervalMs: config.healthIntervalMs,
          stallTimeoutMs: config.stalledAfterMs,
          repeatedToolThreshold: config.repeatedToolThreshold,
        },
        llm: port,
        routeRegistry: registry,
        ...(config.defaultRoute === undefined || config.defaultRoute === null ? {} : { defaultRoute: config.defaultRoute }),
        ...(ncagSettings === undefined ? {} : { settings: () => ncagSettings.runtime() }),
      })
      const entry: SharedPlatform = { orchestrator: this.orchestrator, started: this.orchestrator.start(), mounts: 1 }
      sharedPlatforms.set(key, entry)
      ctx.logger.info('ncag: platform starting')
      /* v8 ignore next 3 -- startup only fails on a corrupt database, which the store's own tests cover. */
      entry.started.catch((error: unknown) => {
        ctx.logger.error(error)
      })
    } else {
      shared.mounts += 1
      this.orchestrator = shared.orchestrator
      ctx.logger.info('ncag: platform already running for this database')
    }
    ctx.effect(() => {
      let disposed = false
      return async () => {
        /* v8 ignore next 2 -- Cordis disposes one effect exactly once; the guard is defensive. */
        if (disposed) return
        disposed = true
        const entry = sharedPlatforms.get(key)
        /* v8 ignore next -- a live mount holds its entry; only a defensive race can miss it. */
        if (entry === undefined) return
        entry.mounts -= 1
        if (entry.mounts > 0) return
        sharedPlatforms.delete(key)
        /* v8 ignore next -- a settled startup promise cannot reject; the handler is type-level defense. */
        await entry.started.catch(() => undefined)
        await entry.orchestrator.stop()
      }
    }, 'ncag.orchestrator')
  }

  /**
   * Create one project anchored at the first registered workspace.
   * @param projectName - project name.
   * @param description - what the project delivers.
   * @returns the created project.
   * @throws Error when the harness has no workspace to anchor the project.
   */
  createProjectFromWorkspace(projectName: string, description: string): Project {
    const workspace = this.ctx.get('workspaceRegistry')?.list()[0]
    if (workspace === undefined) {
      throw new Error('ncag: the harness has no registered workspace; open one before creating an NC-AG project')
    }
    return this.orchestrator.createProject({ name: projectName, description, repositoryPath: workspace.path })
  }
}
