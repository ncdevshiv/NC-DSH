/**
 * Planning: intent analysis, requirement normalization, ambiguity detection
 * and task-graph construction from a user request.
 *
 * The deterministic analyzer derives requirements and tasks from the request
 * statements and the capabilities the profile registry serves. It needs no
 * model route, and it produces the same record types a model-produced plan is
 * validated into, so nothing downstream depends on which analyzer ran.
 * @module @deepseek-ai/dsh-ncag-orchestrator/planner
 */

import { newId } from '@deepseek-ai/dsh-ncag-core'
import type {
  AcceptanceCriterion, Ambiguity, IntentAnalysis, Priority, Requirement, Task, VerificationMethod,
} from '@deepseek-ai/dsh-ncag-core'

/** One planned task before identity and scheduling fields are attached. */
export interface PlannedTask {
  readonly title: string
  readonly description: string
  readonly requiredCapability: string
  /** Indexes into the plan task list that must complete first. */
  readonly dependsOn: readonly number[]
  readonly requiredInputs: readonly string[]
  readonly expectedOutputs: readonly string[]
  readonly acceptanceCriteria: readonly AcceptanceCriterion[]
  readonly requiredReviews: readonly Task['requiredReviews'][number][]
  readonly requiredVerifications: readonly VerificationMethod[]
  readonly priority: Priority
  /** Whether the task delivers a recorded answer instead of a file change. */
  readonly consultation?: boolean
}

/** Complete structured plan for one request. */
export interface ExecutionPlan {
  readonly intent: IntentAnalysis
  readonly ambiguities: readonly Ambiguity[]
  readonly requirements: readonly Omit<Requirement, 'id' | 'requestId' | 'status'>[]
  readonly tasks: readonly PlannedTask[]
  /** Which analyzer produced the plan. */
  readonly source: 'model' | 'deterministic'
  /** The rule set or planner statement behind the plan, recorded in the audit trail. */
  readonly rationale: string
}

/** One capability keyword rule. */
export interface CapabilityRule {
  readonly keywords: readonly string[]
  readonly capability: string
  readonly title: string
  readonly output: string
}

/** Request vocabulary mapped to the capabilities the registry serves. */
export const CAPABILITY_RULES: readonly CapabilityRule[] = [
  { keywords: ['ui', 'interface', 'frontend', 'front end', 'page', 'dashboard', 'screen', 'form', 'component', 'layout'], capability: 'frontend-implementation', title: 'Implement the user interface', output: 'interface implementation' },
  { keywords: ['api', 'endpoint', 'rest', 'graphql', 'contract', 'schema', 'route'], capability: 'api-contract', title: 'Define and implement the API contract', output: 'API contract and implementation' },
  { keywords: ['database', 'migration', 'sql', 'storage', 'persist', 'table'], capability: 'database-design', title: 'Design the data schema and migration', output: 'schema and migration' },
  { keywords: ['backend', 'server', 'service', 'business logic', 'authentication', 'authorization'], capability: 'backend-implementation', title: 'Implement the backend behavior', output: 'backend implementation' },
  { keywords: ['test', 'testing', 'coverage', 'regression', 'qa'], capability: 'functional-verification', title: 'Verify the delivered behavior', output: 'verification report' },
  { keywords: ['security', 'audit', 'permission', 'credential', 'secret', 'threat'], capability: 'security-review', title: 'Audit security and permissions', output: 'security audit' },
  { keywords: ['deploy', 'deployment', 'ci', 'cd', 'pipeline', 'docker', 'container', 'release'], capability: 'deployment', title: 'Prepare deployment configuration', output: 'deployment configuration' },
  { keywords: ['document', 'documentation', 'readme', 'guide', 'manual'], capability: 'documentation', title: 'Write the matching documentation', output: 'documentation' },
  { keywords: ['architecture', 'design', 'module', 'boundary'], capability: 'architecture', title: 'Establish the architecture and interfaces', output: 'architecture description' },
  { keywords: ['performance', 'latency', 'throughput', 'optimize', 'optimise', 'benchmark'], capability: 'performance-analysis', title: 'Measure and optimize performance', output: 'performance report' },
]

/** Lowercase and strip punctuation so keyword matching works on token boundaries. */
const WORD_BOUNDARY = new RegExp('[^a-z0-9-]+', 'g')

/** Normalize free text into space-separated lowercase words. */
function words(text: string): string {
  return ` ${text.toLowerCase().replace(WORD_BOUNDARY, ' ').trim()} `
}

/**
 * Whether one keyword or phrase occurs in the text on word boundaries.
 * @param text - free text to search.
 * @param keyword - single word or multi-word phrase.
 * @returns whether the phrase occurs.
 */
export function mentions(text: string, keyword: string): boolean {
  return words(text).includes(words(keyword))
}

/** Priority inferred from the request's own ordering words. */
function priorityOf(text: string): Priority {
  if (mentions(text, 'critical') || mentions(text, 'urgent') || mentions(text, 'blocker')) return 'P0'
  if (mentions(text, 'asap') || mentions(text, 'important')) return 'P1'
  if (mentions(text, 'low priority') || mentions(text, 'optional')) return 'P3'
  return 'P2'
}

/** Capability each named specialist answers a consultation with. */
const SPECIALIST_CAPABILITIES: readonly { readonly keywords: readonly string[]; readonly capability: string }[] = [
  { keywords: ['architect', 'architecture'], capability: 'architecture' },
  { keywords: ['backend'], capability: 'backend-implementation' },
  { keywords: ['frontend', 'ui engineer'], capability: 'frontend-implementation' },
  { keywords: ['api engineer', 'api specialist'], capability: 'api-contract' },
  { keywords: ['database', 'schema engineer'], capability: 'database-design' },
  { keywords: ['devops', 'infrastructure'], capability: 'devops' },
  { keywords: ['documentation', 'technical writer'], capability: 'documentation' },
  { keywords: ['qa', 'quality'], capability: 'functional-verification' },
  { keywords: ['e2e', 'end to end test'], capability: 'e2e-testing' },
  { keywords: ['security', 'auditor'], capability: 'security-review' },
  { keywords: ['reviewer', 'code review'], capability: 'code-review' },
  { keywords: ['critiquer', 'griller'], capability: 'critique' },
  { keywords: ['integration engineer', 'merge engineer'], capability: 'integration' },
  { keywords: ['performance', 'profiler'], capability: 'performance-analysis' },
  { keywords: ['product analyst', 'requirements analyst'], capability: 'requirements-analysis' },
]

/**
 * The capability that answers one consultation.
 *
 * The specialist the request names is looked up first, so "consult the security
 * auditor" is answered by the security auditor rather than by whichever
 * capability happens to be registered first. A request that names no known
 * specialist falls back to the general-purpose agent.
 * @param requestText - the complete request text.
 * @returns the capability to schedule.
 */
export function consultationCapability(requestText: string): string {
  for (const entry of SPECIALIST_CAPABILITIES) {
    if (entry.keywords.some(keyword => mentions(requestText, keyword))) return entry.capability
  }
  return 'general-implementation'
}

/** Statements that ask for a specialist to be consulted on the request. */
const HELP_KEYWORDS: readonly string[] = ['consult', 'consultation', 'ask the', 'seek expert', 'second opinion', 'hand off', 'handoff', 'take over']

/**
 * Whether one statement asks for a specialist consultation or a handoff.
 *
 * Such a statement is a coordination request, not a deliverable the specialist
 * implements on its own: the orchestrator answers it by consulting a
 * registered specialist and recording the answer against the active task.
 * @param statement - one request statement.
 * @returns whether the statement asks for help.
 */
export function mentionsHelp(statement: string): boolean {
  return HELP_KEYWORDS.some(keyword => mentions(statement, keyword))
}

/**
 * Statements that describe constraints, motivation or outcomes rather than
 * deliverable work, and so produce no task of their own.
 * @param statement - one request statement.
 * @returns whether the statement is descriptive.
 */
export function isDescriptive(statement: string): boolean {
  return mentions(statement, 'because') || mentions(statement, 'so that') || mentions(statement, 'in order to')
}

/** Split a request into statements: one per line or sentence. */
function statementsOf(text: string): string[] {
  const parts = text.split(/[\n;]+/)
  const sentences: string[] = []
  for (const part of parts) {
    for (const sentence of part.split('. ')) {
      const trimmed = sentence.trim().replace(/\.$/, '')
      if (trimmed.length > 0) sentences.push(trimmed)
    }
  }
  return sentences
}
/**
 * Build the deterministic plan for one request.
 * @param requestText - the user request exactly as received.
 * @param projectName - project the request belongs to.
 * @param capabilities - capabilities a registered profile can serve.
 * @param hasArchitectureContext - whether the project already holds approved architecture.
 * @param checks - executable command per verification method this project supports.
 * @returns the complete plan.
 */
export function planDeterministically(
  requestText: string,
  projectName: string,
  capabilities: readonly string[],
  hasArchitectureContext: boolean,
  checks: Partial<Record<VerificationMethod, string>>,
): ExecutionPlan {
  const priority = priorityOf(requestText)
  const statements = statementsOf(requestText)
  const motivation = statements.find(item => mentions(item, 'because') || mentions(item, 'so that')) ?? ''
  const technologies = ['typescript', 'javascript', 'node', 'react', 'vue', 'sqlite', 'postgres', 'python', 'go', 'rust', 'rest', 'graphql', 'css', 'html']
    .filter(technology => mentions(requestText, technology))
  const intent: IntentAnalysis = {
    goal: statements[0] ?? requestText.trim(),
    motivation,
    /* v8 ignore next 2 -- a multi-statement request always has a last statement; the fallback is type-level defense. */
    outcomes: statements.length > 1 ? [statements[statements.length - 1] ?? ''] : [statements[0] ?? requestText.trim()],
    constraints: statements.filter(item => mentions(item, 'must') || mentions(item, 'required') || mentions(item, 'never')) ,
    technologies,
    scope: `Deliver the requested change inside project ${projectName}.`,
    priority,
    quality: 'Every acceptance criterion is verified by executed evidence before the task may complete.',
    security: mentions(requestText, 'security') || mentions(requestText, 'auth') || mentions(requestText, 'permission')
      ? ['The request touches security-sensitive behavior; an independent audit is required.']
      : [],
  }
  const helpRequested = statements.some(statement => !isDescriptive(statement) && mentionsHelp(statement))
  const ambiguities: Ambiguity[] = []
  if (helpRequested) {
    ambiguities.push({
      id: newId('correlation'),
      description: 'The request asks for a consultation rather than for material work.',
      material: false,
      assumption: 'The consultation is answered by a registered specialist and recorded against the project before the request is reported complete.',
      question: null,
    })
  }
  /* v8 ignore next -- a single statement implies a first statement, so the empty fallback is type-level defense. */
  if (statements.length === 1 && (statements[0] ?? '').trim().split(' ').length < 4) {
    ambiguities.push({
      id: newId('correlation'),
      description: 'The request is very short and states no expected outcome.',
      material: false,
      assumption: 'The requested behavior is delivered as a working implementation with executed tests.',
      question: null,
    })
  }
  const requirements: Omit<Requirement, 'id' | 'requestId' | 'status'>[] = statements.map((statement, index) => ({
    description: statement,
    category: mentions(statement, 'security') ? 'security' : 'functional',
    priority,
    source: `request statement ${index + 1}`,
    dependencies: [],
    acceptanceCriteria: [{
      id: `ac-${index + 1}`,
      text: 'The behavior described by the requirement is present and proven by an executed check.',
      verification: 'unit-test',
    }],
  }))
  const tasks: PlannedTask[] = []
  // The architecture task covers the request statement that names architecture,
  // so the capability rules below must not add a second task for that capability:
  // a task depending on itself can never become ready.
  const architectureFirst = !hasArchitectureContext && !helpRequested && mentions(requestText, 'architecture')
  if (architectureFirst) {
    tasks.push({
      title: 'Establish the architecture and interfaces',
      description: 'Define module boundaries, the interfaces between them, and the failure behavior of each boundary before implementation starts.',
      requiredCapability: 'architecture',
      dependsOn: [],
      requiredInputs: ['normalized requirements'],
      expectedOutputs: ['architecture description'],
      acceptanceCriteria: [{ id: 'arch-1', text: 'Every boundary names its owner, input, output and failure behavior.', verification: 'manual-review', ...(checks['manual-review'] === undefined ? {} : { command: checks['manual-review'] }) }],
      requiredReviews: ['critique'],
      requiredVerifications: ['manual-review'],
      priority,
    })
  }
  const covered = new Set<string>(architectureFirst ? ['architecture'] : [])
  const seen = new Set<string>()
  for (const rule of CAPABILITY_RULES) {
    // A consultation names its specialist; the specialist answers the question,
    // so keyword matches in the question never schedule the specialist's own
    // deliverable work.
    if (helpRequested) break
    if (!rule.keywords.some(keyword => mentions(requestText, keyword))) continue
    if (covered.has(rule.capability)) continue
    /* v8 ignore next -- CAPABILITY_RULES declare distinct capabilities, so a capability never repeats. */
    if (seen.has(rule.capability)) continue
    if (!capabilities.includes(rule.capability)) continue
    seen.add(rule.capability)
    const dependsOn = architectureFirst ? [0] : []
    tasks.push({
      title: rule.title,
      description: `${rule.title} for the request: ${requestText.trim()}`,
      requiredCapability: rule.capability,
      dependsOn,
      requiredInputs: architectureFirst ? ['architecture description'] : ['normalized requirements'],
      expectedOutputs: [rule.output],
      acceptanceCriteria: [{
        id: `ac-${rule.capability}`,
        text: `The ${rule.output} is present and every command it required ran with recorded output.`,
        verification: 'unit-test',
      }],
      requiredReviews: ['code-review'],
      requiredVerifications: ['unit-test'],
      priority,
    })
  }
  // A request no capability rule claims is still real work: it becomes one
  // implementation task, waiting for the architecture when the request asked
  // for architecture first. A consultation request becomes one specialist task
  // whose deliverable is the recorded answer, not a change on disk.
  if (seen.size === 0) {
    const callback = helpRequested ? consultationCapability(requestText) : 'general-implementation'
    const declared = helpRequested && capabilities.includes(callback) ? callback : undefined
    const capability = declared ?? (capabilities.includes('general-implementation') ? 'general-implementation' : (capabilities[0] ?? 'general-implementation'))
    tasks.push({
      title: helpRequested ? 'Answer the specialist consultation' : 'Implement the requested change',
      description: requestText.trim(),
      requiredCapability: capability,
      dependsOn: architectureFirst ? [0] : [],
      requiredInputs: architectureFirst ? ['architecture description'] : ['the request text'],
      expectedOutputs: helpRequested ? ['consultation answer'] : ['implementation'],
      acceptanceCriteria: [{
        id: 'ac-1',
        text: helpRequested
          ? 'The specialist answer is recorded against the project and names what it examined.'
          : 'The requested behavior works and is proven by an executed check.',
        // A consultation produces a recorded answer, not a change on disk, so
        // the criterion is the independent review of that answer.
        verification: helpRequested ? 'manual-review' : 'unit-test',
        ...(helpRequested || checks['unit-test'] === undefined ? {} : { command: checks['unit-test'] }),
      }],
      requiredReviews: ['code-review'],
      requiredVerifications: [helpRequested ? 'manual-review' : 'unit-test'],
      priority,
      ...(helpRequested ? { consultation: true } : {}),
    })
  }
  return {
    intent,
    ambiguities,
    requirements,
    tasks,
    source: 'deterministic',
    rationale: 'keyword and statement analysis of the request against the registered capabilities',
  }
}
