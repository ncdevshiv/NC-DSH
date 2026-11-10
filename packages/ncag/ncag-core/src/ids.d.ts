/**
 * Branded NC-AG identifiers and their minting helpers.
 *
 * Every cross-module identifier is a {@link Branded} string, so a project id
 * cannot be passed where a task id is expected. Minting is centralized here:
 * callers never invent identifier text, which keeps the store's uniqueness
 * guarantees meaningful.
 * @module @deepseek-ai/dsh-ncag-core/ids
 */
import type { Branded } from '@deepseek-ai/dsh-brand';
/** Identifier of one NC-AG project. */
export type ProjectId = Branded<'NcagProjectId'>;
/** Identifier of one user request inside a project. */
export type RequestId = Branded<'NcagRequestId'>;
/** Identifier of one normalized requirement. */
export type RequirementId = Branded<'NcagRequirementId'>;
/** Identifier of one task in a project's DAG. */
export type TaskId = Branded<'NcagTaskId'>;
/** Identifier of one agent instance. */
export type AgentId = Branded<'NcagAgentId'>;
/** Identifier of one agent profile in the profile registry. */
export type ProfileId = Branded<'NcagProfileId'>;
/** Identifier of one message in the durable communication log. */
export type MessageId = Branded<'NcagMessageId'>;
/** Identifier of one decision record. */
export type DecisionId = Branded<'NcagDecisionId'>;
/** Identifier of one review finding. */
export type FindingId = Branded<'NcagFindingId'>;
/** Identifier of one resource lease. */
export type LeaseId = Branded<'NcagLeaseId'>;
/** Identifier of one submitted change set awaiting integration. */
export type ChangeSetId = Branded<'NcagChangeSetId'>;
/** Identifier of one durable audit record. */
export type AuditId = Branded<'NcagAuditId'>;
/** Identifier of one checkpoint that restores execution state. */
export type CheckpointId = Branded<'NcagCheckpointId'>;
/** Identifier correlating one logical operation across layers. */
export type CorrelationId = Branded<'NcagCorrelationId'>;
/** Identifier families that can be minted. */
export type IdKind = 'project' | 'request' | 'requirement' | 'task' | 'agent' | 'profile' | 'message' | 'decision' | 'finding' | 'lease' | 'changeset' | 'audit' | 'checkpoint' | 'correlation';
/**
 * Mint a fresh identifier of one family.
 * @param kind - identifier family to mint.
 * @returns the prefixed UUID text carrying the family's brand.
 */
/** The branded identifier type each family produces. */
export interface IdFor {
    project: ProjectId;
    request: RequestId;
    requirement: RequirementId;
    task: TaskId;
    agent: AgentId;
    profile: ProfileId;
    message: MessageId;
    decision: DecisionId;
    finding: FindingId;
    lease: LeaseId;
    changeset: ChangeSetId;
    audit: AuditId;
    checkpoint: CheckpointId;
    correlation: CorrelationId;
}
/**
 * Mint a fresh identifier of one family.
 * @param kind - identifier family to mint.
 * @returns the prefixed UUID text carrying the family's brand.
 */
export declare function newId<K extends IdKind>(kind: K): IdFor[K];
/**
 * Adopt existing identifier text as one family's brand.
 * @param kind - identifier family the text belongs to.
 * @param text - identifier text previously produced by {@link newId} or carried by a durable record.
 * @returns the same text with the family's brand.
 */
export declare function adoptId<K extends IdKind>(kind: K, text: string): IdFor[K];
//# sourceMappingURL=ids.d.ts.map