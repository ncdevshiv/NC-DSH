/**
 * NC-AG durable record vocabulary: projects, requests, requirements, tasks,
 * agents, messages, decisions, findings, leases, change sets, usage and audit
 * records, plus the enums their state machines move between.
 *
 * These are the only values the platform persists and exchanges between its
 * modules; runtime behavior lives in the modules that own each record lifecycle.
 * @module @deepseek-ai/dsh-ncag-core/types
 */
import type { AgentId, ChangeSetId, CorrelationId, DecisionId, FindingId, LeaseId, MessageId, ProfileId, ProjectId, RequestId, RequirementId, TaskId } from './ids.ts';
/** UTC instant recorded with millisecond precision, as an ISO-8601 string. */
export type Instant = string;
/** Milliseconds since the Unix epoch, used for lease and deadline arithmetic. */
export type InstantMs = number;
/** Every lifecycle state an agent instance can occupy. */
export type AgentStatus = 'CREATED' | 'INITIALIZING' | 'STARTING' | 'IDLE' | 'ASSIGNED' | 'WORKING' | 'WAITING' | 'BLOCKED' | 'HELP_REQUESTED' | 'REVIEWING' | 'HANDOFF_PENDING' | 'RECOVERING' | 'STOPPING' | 'STOPPED' | 'FAILED';
/** States from which an agent still accepts work or observation. */
export declare const LIVE_AGENT_STATUSES: readonly AgentStatus[];
/** Every lifecycle state a task can occupy. */
export type TaskStatus = 'CREATED' | 'ANALYZING' | 'PLANNED' | 'WAITING_APPROVAL' | 'READY' | 'ASSIGNED' | 'IN_PROGRESS' | 'BLOCKED' | 'REVIEW_PENDING' | 'VERIFICATION_PENDING' | 'COMPLETED' | 'REJECTED' | 'FAILED' | 'CANCELLED';
/** Task states in which the scheduler may consider work done. */
export declare const TERMINAL_TASK_STATUSES: readonly TaskStatus[];
/** Task priority, highest first. */
export type Priority = 'P0' | 'P1' | 'P2' | 'P3';
/**
 * One reusable specialization. Profiles are the registry entries; instances
 * are spawned from them and carry their own identity, workspace and history.
 */
export interface AgentProfile {
    /** Stable registry identifier. */
    readonly id: ProfileId;
    /** Registry key used by planning to select a capability. */
    readonly key: string;
    /** Human-readable role name. */
    readonly role: string;
    /** Identity statement the agent receives in its system prompt. */
    readonly identity: string;
    /** Communication style and reasoning posture. */
    readonly personality: string;
    /** Expertise statements. */
    readonly expertise: readonly string[];
    /** Capability tags planning matches against task requirements. */
    readonly capabilities: readonly string[];
    /** Tool names this profile may execute. An empty list means no tools. */
    readonly toolNames: readonly string[];
    /** What the profile is responsible for. */
    readonly responsibilities: readonly string[];
    /** What the profile must not do. */
    readonly restrictions: readonly string[];
    /** Permission grants checked before every tool call. */
    readonly permissions: readonly NcagPermission[];
    /** Inputs a task must supply before assignment succeeds. */
    readonly requiredInputs: readonly string[];
    /** Deliverable kinds the profile produces. */
    readonly deliverables: readonly string[];
    /** Quality bar stated to the agent and enforced by review. */
    readonly qualityStandard: string;
    /** Conditions under which the agent must escalate rather than proceed. */
    readonly escalationRules: readonly string[];
    /** Measurements the orchestrator records for this profile. */
    readonly performanceMetrics: readonly string[];
    /** Model capability tier this profile prefers; the runtime resolves it to a provider route. */
    readonly defaultTier: ModelTier;
    /** Built-in profiles ship with the platform; created profiles are additions. */
    readonly builtIn: boolean;
}
/** Capability tiers the model router resolves to provider routes. */
export type ModelTier = 'reasoning' | 'standard' | 'fast';
/** Provider route an agent profile uses for model calls. */
export interface ModelRoute {
    /** Registered LLM provider name. */
    readonly provider: string;
    /** Provider model id. */
    readonly model: string;
    /** Optional reasoning effort identifier forwarded to the adapter. */
    readonly reasoningEffort?: string;
}
/** Permission families enforced before tool execution. */
export type NcagPermission = 'workspace:read' | 'workspace:write' | 'git:write' | 'shell:execute' | 'web:access' | 'communication:send' | 'resource:lease' | 'review:submit' | 'decision:propose';
/** One spawned agent. Instances are durable; a restart resumes them. */
export interface AgentInstance {
    readonly id: AgentId;
    readonly profileId: ProfileId;
    readonly profileKey: string;
    readonly projectId: ProjectId;
    readonly taskId: TaskId | null;
    readonly status: AgentStatus;
    readonly createdAt: Instant;
    readonly updatedAt: Instant;
    /** Last heartbeat written by the agent loop. */
    readonly lastHeartbeatAt: Instant;
    /** Last time the agent produced a model token or tool result. */
    readonly lastActivityAt: Instant;
    /** Isolated Git worktree this agent owns. */
    readonly workspacePath: string | null;
    readonly branch: string | null;
    readonly model: ModelRoute;
    readonly capabilities: readonly string[];
    /** Accumulated usage for this instance. */
    readonly usage: UsageTotals;
    /** Consecutive failed attempts, used by the recovery manager. */
    readonly failureCount: number;
    /** Set when the agent is blocked, waiting, or requesting help. */
    readonly statusReason: string | null;
    /** Correlation id of the turn currently executing, if any. */
    readonly activeCorrelationId: CorrelationId | null;
    /** Number of model steps consumed by this instance. */
    readonly steps: number;
}
/** Accumulated token and cost accounting. */
export interface UsageTotals {
    readonly inputTokens: number;
    readonly outputTokens: number;
    readonly cachedTokens: number;
    readonly costUsd: number;
    readonly calls: number;
}
/** Autonomy posture configured per project. */
export interface AutonomyPolicy {
    /** Whether material architectural decisions require the user approval. */
    readonly requireArchitecturalApproval: boolean;
    /** Whether the orchestrator may spawn agents without a plan approval. */
    readonly autonomousSpawning: boolean;
    /** Whether a structured disagreement must precede a conflicting choice. */
    readonly challengeOnConflict: boolean;
    /** Whether completion requires independent verification evidence. */
    readonly requireVerification: boolean;
    /** Hard cost ceiling in US dollars; null means unlimited. */
    readonly maxCostUsd: number | null;
    /** Maximum number of simultaneously live agents in this project. */
    readonly maxConcurrentAgents: number;
    /** Maximum agents assigned to one task, including helpers. */
    readonly maxAgentsPerTask: number;
    /** Maximum retries per task before escalation. */
    readonly maxRetries: number;
    /** Maximum review cycles per task before escalation. */
    readonly maxReviewCycles: number;
    /** Wall-clock ceiling for one task in milliseconds. */
    readonly maxTaskDurationMs: number;
    /** Model selection policy; economical prefers the cheapest capable route. */
    readonly modelPolicy: 'economical' | 'balanced' | 'capability-first';
    /** Preferred provider names in priority order. */
    readonly preferredProviders: readonly string[];
}
/** Project-level status. */
export type ProjectStatus = 'ACTIVE' | 'PAUSED' | 'COMPLETED' | 'FAILED' | 'ARCHIVED';
/** One NC-AG project. Projects never share task state. */
export interface Project {
    readonly id: ProjectId;
    readonly name: string;
    readonly description: string;
    /** Root of the Git repository agents work in. */
    readonly repositoryPath: string;
    /** Base branch integrated changes land on. */
    readonly baseBranch: string;
    readonly status: ProjectStatus;
    readonly createdAt: Instant;
    readonly updatedAt: Instant;
    readonly autonomy: AutonomyPolicy;
    /** Project-specific instructions every agent in this project receives. */
    readonly instructions: string;
    /** Stable project decisions (approved architecture, contracts). */
    readonly context: ProjectContext;
    /** Executable command per verification method this project supports; an absent method cannot be executed. */
    readonly checks: Partial<Record<VerificationMethod, string>>;
    readonly usage: UsageTotals;
}
/** Accumulated shared context handed to agents in relevant slices. */
export interface ProjectContext {
    /** Approved architecture statements. */
    readonly architecture: readonly ContextFact[];
    /** Approved interface contracts. */
    readonly contracts: readonly ContextFact[];
    /** Known constraints and limitations. */
    readonly constraints: readonly ContextFact[];
    /** Outstanding work statements. */
    readonly outstanding: readonly ContextFact[];
    /** Known problems with their current status. */
    readonly knownProblems: readonly ContextFact[];
}
/** One durable context statement with provenance. */
export interface ContextFact {
    readonly id: string;
    readonly text: string;
    readonly source: string;
    readonly recordedAt: Instant;
}
/** A user request as received, before and after normalization. */
export interface UserRequest {
    readonly id: RequestId;
    readonly projectId: ProjectId;
    readonly rawText: string;
    readonly createdAt: Instant;
    /** Structured intent analysis. */
    readonly intent: IntentAnalysis;
    /** Ambiguities that materially affect execution. */
    readonly ambiguities: readonly Ambiguity[];
    /** Requirements normalized from the request. */
    readonly requirements: readonly Requirement[];
    readonly status: RequestStatus;
}
/** Request status through analysis and execution. */
export type RequestStatus = 'RECEIVED' | 'ANALYZING' | 'PLANNED' | 'AWAITING_DECISION' | 'EXECUTING' | 'DELIVERED' | 'FAILED' | 'CANCELLED';
/** Structured intent analysis produced by the planning subsystem. */
export interface IntentAnalysis {
    /** What the user wants, in one paragraph. */
    readonly goal: string;
    /** Why they want it, when the request states a motivation. */
    readonly motivation: string;
    /** Expected outcomes stated by the request. */
    readonly outcomes: readonly string[];
    /** Constraints the request states or implies. */
    readonly constraints: readonly string[];
    /** Technologies the request requires. */
    readonly technologies: readonly string[];
    /** Scope boundaries. */
    readonly scope: string;
    /** Ordering priority expressed by the user. */
    readonly priority: Priority;
    /** Quality expectations. */
    readonly quality: string;
    /** Security implications identified during analysis. */
    readonly security: readonly string[];
}
/** One ambiguity found during analysis. */
export interface Ambiguity {
    readonly id: string;
    readonly description: string;
    /** Whether the ambiguity changes what gets built. */
    readonly material: boolean;
    /** Assumption recorded when the ambiguity is not material. */
    readonly assumption: string | null;
    /** Question asked of the user when the ambiguity is material. */
    readonly question: string | null;
}
/** One normalized requirement with traceability to its source request. */
export interface Requirement {
    readonly id: RequirementId;
    readonly requestId: RequestId;
    readonly description: string;
    readonly category: RequirementCategory;
    readonly priority: Priority;
    readonly source: string;
    readonly dependencies: readonly RequirementId[];
    readonly acceptanceCriteria: readonly AcceptanceCriterion[];
    readonly status: RequirementStatus;
}
/** Requirement categories. */
export type RequirementCategory = 'functional' | 'non-functional' | 'security' | 'operational' | 'documentation' | 'quality';
/** Requirement status. */
export type RequirementStatus = 'PROPOSED' | 'APPROVED' | 'IMPLEMENTED' | 'VERIFIED' | 'REJECTED';
/** One checkable acceptance criterion. */
export interface AcceptanceCriterion {
    readonly id: string;
    readonly text: string;
    /** How the criterion is verified. */
    readonly verification: VerificationMethod;
    /** Command executed in the agent workspace for command-based methods. */
    readonly command?: string;
}
/** Verification methods the platform can actually execute. */
export type VerificationMethod = 'unit-test' | 'integration-test' | 'e2e-test' | 'typecheck' | 'lint' | 'build' | 'manual-review' | 'security-review' | 'command';
/** One executable unit of work in a project task graph. */
export interface Task {
    readonly id: TaskId;
    readonly projectId: ProjectId;
    readonly requestId: RequestId | null;
    readonly requirementIds: readonly RequirementId[];
    readonly title: string;
    readonly description: string;
    /** Capability tag the responsible agent profile must provide. */
    readonly requiredCapability: string;
    /** Agent currently assigned, if any. */
    readonly assignedAgentId: AgentId | null;
    /** Tasks that must complete before this task may start. */
    readonly dependencyIds: readonly TaskId[];
    /** Inputs the task expects from its dependencies or project context. */
    readonly requiredInputs: readonly string[];
    /** Artifacts the task must produce. */
    readonly expectedOutputs: readonly string[];
    readonly acceptanceCriteria: readonly AcceptanceCriterion[];
    /** Permissions the assigned agent needs for this task. */
    readonly requiredPermissions: readonly NcagPermission[];
    /** Context slice identifiers relevant to this task. */
    readonly contextRefs: readonly string[];
    readonly priority: Priority;
    /** Absolute deadline in milliseconds since the epoch. */
    readonly deadlineMs: InstantMs | null;
    /** Independent reviewers required before verification. */
    readonly requiredReviews: readonly ReviewKind[];
    /** Verification methods required before completion. */
    readonly requiredVerifications: readonly VerificationMethod[];
    readonly status: TaskStatus;
    /** Nesting depth used by the hierarchy view. */
    readonly depth: number;
    readonly createdAt: Instant;
    readonly updatedAt: Instant;
    readonly startedAt: Instant | null;
    readonly completedAt: Instant | null;
    /** Retry counter maintained by the scheduler. */
    readonly attempts: number;
    /** Number of review cycles consumed. */
    readonly reviewCycles: number;
    /**
     * Whether this task delivers a written answer rather than a file change.
     *
     * A consultation task is judged on the recorded review of its answer: it
     * produces no workspace change, so a change-set requirement would reject
     * every correct answer.
     */
    readonly consultation: boolean;
    /** Reason for the current status, when it is BLOCKED, REJECTED or FAILED. */
    readonly statusReason: string | null;
}
/** Review kinds an independent agent can perform. */
export type ReviewKind = 'code-review' | 'critique' | 'security-audit' | 'acceptance';
/** Structured message types exchanged through the communication log. */
export type MessageType = 'INFORMATION' | 'TASK_UPDATE' | 'HELP_REQUEST' | 'HELP_RESPONSE' | 'BLOCKER' | 'RESOURCE_REQUEST' | 'RESOURCE_RELEASE' | 'CONTRACT_PROPOSAL' | 'CONTRACT_CHANGE' | 'ARCHITECTURE_PROPOSAL' | 'ARCHITECTURE_CONFLICT' | 'REVIEW_REQUEST' | 'REVIEW_FINDING' | 'HANDOFF' | 'ESCALATION' | 'TASK_COMPLETION' | 'FAILURE' | 'SYSTEM_EVENT';
/** Delivery states a message moves through. */
export type DeliveryStatus = 'PENDING' | 'DELIVERED' | 'ACKNOWLEDGED' | 'EXPIRED' | 'DEAD_LETTERED';
/** Where a message is addressed. */
export type MessageTarget = {
    readonly kind: 'agent';
    readonly agentId: AgentId;
} | {
    readonly kind: 'channel';
    readonly channel: string;
} | {
    readonly kind: 'orchestrator';
};
/** One durable message. */
export interface NcagMessage {
    readonly id: MessageId;
    /** UTC timestamp in ISO-8601 form. */
    readonly timestamp: Instant;
    /** Sending agent, or the orchestrator identity. */
    readonly sender: AgentId | null;
    readonly target: MessageTarget;
    readonly projectId: ProjectId;
    readonly taskId: TaskId | null;
    readonly type: MessageType;
    readonly subject: string;
    readonly content: string;
    /** Durable records this message refers to. */
    readonly contextRefs: readonly string[];
    readonly correlationId: CorrelationId;
    readonly priority: 'low' | 'normal' | 'high' | 'critical';
    /** Whether the recipient must acknowledge before the sender proceeds. */
    readonly requiresAck: boolean;
    readonly deliveryStatus: DeliveryStatus;
    readonly deliveredAt: Instant | null;
    readonly acknowledgedAt: Instant | null;
    /** Number of delivery attempts made so far. */
    readonly attempts: number;
    /** Expiry in milliseconds since the epoch; delivery stops after it. */
    readonly expiresAtMs: InstantMs | null;
}
/** Decision class per the platform autonomy contract. */
export type DecisionCategory = 'autonomous' | 'technical-recommendation' | 'material-architectural' | 'user-reserved';
/** Lifecycle of a recorded decision. */
export type DecisionStatus = 'PROPOSED' | 'AWAITING_USER' | 'ACCEPTED' | 'OVERRIDDEN' | 'SUPERSEDED' | 'WITHDRAWN';
/** One approach under comparison, with cost and risk implications. */
export interface DecisionAlternative {
    readonly label: string;
    readonly description: string;
    readonly advantages: readonly string[];
    readonly disadvantages: readonly string[];
    readonly costImplication: string;
    readonly complexityImplication: string;
    readonly reliabilityImplication: string;
    readonly securityImplication: string;
    readonly maintenanceImplication: string;
}
/**
 * The structured disagreement the platform presents before a conflicting
 * choice: what was requested, what the platform believes, why they differ,
 * the recommendation, an objective comparison, the evidence, and the exact
 * decision question.
 */
export interface DebateCase {
    /** A. What the user is asking for, restated accurately. */
    readonly requested: string;
    /** B. What NC-AG believes and proposes. */
    readonly belief: string;
    /** C. The exact conflict. */
    readonly conflict: string;
    /** D. The recommended alternative. */
    readonly recommendation: string;
    /** E. Both approaches compared on the same axes. */
    readonly alternatives: readonly DecisionAlternative[];
    /** F. Evidence and reasoning, each item traceable. */
    readonly evidence: readonly string[];
    /** G. The unambiguous decision question put to the user. */
    readonly question: string;
}
/** Permanent record of one significant decision. */
export interface DecisionRecord {
    readonly id: DecisionId;
    readonly projectId: ProjectId;
    readonly taskId: TaskId | null;
    readonly category: DecisionCategory;
    readonly author: string;
    readonly problem: string;
    readonly debate: DebateCase | null;
    readonly recommendation: string;
    /** The choice that was actually taken. */
    readonly outcome: string;
    /** User rationale, when the user decided. */
    readonly rationale: string | null;
    /** Recommendation the user rejected, when they overrode it. */
    readonly rejectedRecommendation: string | null;
    readonly consequences: readonly string[];
    readonly affectedModules: readonly string[];
    readonly affectedTaskIds: readonly TaskId[];
    readonly status: DecisionStatus;
    readonly createdAt: Instant;
    readonly decidedAt: Instant | null;
    /** Ordered history of review or user actions on this decision. */
    readonly reviewHistory: readonly string[];
}
/** Severity of a review finding. */
export type FindingSeverity = 'CRITICAL' | 'HIGH' | 'MEDIUM' | 'LOW' | 'INFORMATIONAL';
/** Lifecycle of a review finding. */
export type FindingStatus = 'OPEN' | 'FIXED' | 'ACCEPTED' | 'DISMISSED';
/** One evidence-backed review finding. */
export interface ReviewFinding {
    readonly id: FindingId;
    readonly taskId: TaskId;
    readonly projectId: ProjectId;
    /** Reviewing agent that produced the finding. */
    readonly reviewerAgentId: AgentId;
    /** Review kind whose evidence produced the finding. */
    readonly reviewKind: ReviewKind;
    readonly severity: FindingSeverity;
    readonly description: string;
    readonly evidence: string;
    /** File or artifact the finding refers to. */
    readonly artifact: string;
    readonly impact: string;
    readonly recommendation: string;
    readonly verificationMethod: string;
    readonly status: FindingStatus;
    readonly createdAt: Instant;
    readonly resolvedAt: Instant | null;
}
/** One executed independent review, recorded whether or not it raised findings. */
export interface ReviewRun {
    readonly id: string;
    readonly projectId: ProjectId;
    readonly taskId: TaskId;
    readonly reviewKind: ReviewKind;
    /** Reviewer agent that executed the review. */
    readonly reviewerAgentId: AgentId;
    /** One-line outcome the reviewer reported. */
    readonly summary: string;
    /** Number of findings the review raised. */
    readonly findings: number;
    /** Whether the review rejected the delivery. */
    readonly rejected: boolean;
    readonly createdAt: Instant;
}
/** Outcome of one executed verification check. */
export type VerificationStatus = 'PASSED' | 'FAILED' | 'ERROR';
/** Evidence recorded for one acceptance criterion. */
export interface VerificationEvidence {
    readonly taskId: TaskId;
    readonly criterionId: string;
    readonly method: VerificationMethod;
    readonly status: VerificationStatus;
    /** Exact command executed, when the method runs a command. */
    readonly command: string | null;
    readonly exitCode: number | null;
    /** Captured output, clipped to the configured budget. */
    readonly output: string;
    readonly recordedAt: Instant;
    /** Agent that produced the evidence. */
    readonly producedBy: AgentId | null;
}
/** Lease lifecycle. */
export type LeaseStatus = 'ACTIVE' | 'RELEASED' | 'EXPIRED' | 'REVOKED';
/** One resource lease over a workspace path. */
export interface ResourceLease {
    readonly id: LeaseId;
    readonly projectId: ProjectId;
    /** Repository-relative path the lease covers. */
    readonly resourcePath: string;
    readonly mode: 'read' | 'write';
    readonly agentId: AgentId;
    readonly taskId: TaskId | null;
    /** Monotonic fencing token; a stale holder must not act on an expired lease. */
    readonly fencingToken: number;
    readonly acquiredAtMs: InstantMs;
    readonly renewedAtMs: InstantMs;
    readonly expiresAtMs: InstantMs;
    readonly status: LeaseStatus;
    /** Why the resource is held; shown to waiting agents. */
    readonly reason: string;
}
/** Lifecycle of a submitted change set. */
export type ChangeSetStatus = 'SUBMITTED' | 'VALIDATING' | 'REJECTED' | 'CONFLICT' | 'MERGED';
/** One agent workspace submission awaiting integration. */
export interface ChangeSet {
    readonly id: ChangeSetId;
    readonly projectId: ProjectId;
    readonly taskId: TaskId;
    readonly agentId: AgentId;
    readonly workspacePath: string;
    readonly branch: string;
    /** Commit the workspace branched from. */
    readonly baseCommit: string;
    /** Commit produced by the workspace, when one exists. */
    readonly headCommit: string | null;
    readonly changedFiles: readonly string[];
    readonly message: string;
    readonly status: ChangeSetStatus;
    readonly submittedAt: Instant;
    /** Findings produced by integration validation. */
    readonly report: readonly string[];
    /** Commit recorded on the base branch after a successful integration. */
    readonly integratedCommit: string | null;
}
/** One isolated Git worktree owned by an agent. */
export interface WorkspaceRecord {
    readonly agentId: AgentId;
    readonly projectId: ProjectId;
    readonly taskId: TaskId | null;
    readonly path: string;
    readonly branch: string;
    readonly baseCommit: string;
    readonly createdAt: Instant;
    readonly releasedAt: Instant | null;
}
/** Audit categories recorded for every externally visible action. */
export type AuditCategory = 'request' | 'planning' | 'agent' | 'task' | 'message' | 'resource' | 'tool' | 'code' | 'review' | 'decision' | 'recovery' | 'cost' | 'delivery' | 'security';
/** One tamper-evident audit record. */
export interface AuditRecord {
    readonly id: string;
    /** Monotonic sequence within the database; gaps indicate lost records. */
    readonly sequence: number;
    readonly at: Instant;
    readonly category: AuditCategory;
    readonly action: string;
    readonly actor: string;
    readonly projectId: ProjectId | null;
    readonly taskId: TaskId | null;
    readonly agentId: AgentId | null;
    readonly correlationId: CorrelationId;
    /** Structured detail; never contains credentials. */
    readonly detail: Record<string, unknown>;
    /** Hash of the previous record in the chain. */
    readonly previousHash: string;
    /** Hash over this record and {@link previousHash}. */
    readonly hash: string;
}
/** One timeline event surfaced by the dashboard. */
export interface PlatformEvent {
    readonly sequence: number;
    readonly at: Instant;
    readonly kind: string;
    readonly projectId: ProjectId | null;
    readonly taskId: TaskId | null;
    readonly agentId: AgentId | null;
    readonly summary: string;
    readonly data: Record<string, unknown>;
}
//# sourceMappingURL=types.d.ts.map