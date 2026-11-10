/**
 * Branded NC-AG identifiers and their minting helpers.
 *
 * Every cross-module identifier is a {@link Branded} string, so a project id
 * cannot be passed where a task id is expected. Minting is centralized here:
 * callers never invent identifier text, which keeps the store's uniqueness
 * guarantees meaningful.
 * @module @deepseek-ai/dsh-ncag-core/ids
 */
import { randomUUID } from 'node:crypto';
import { brandString } from '@deepseek-ai/dsh-brand';
const PREFIX = {
    project: 'prj',
    request: 'req',
    requirement: 'rq',
    task: 'tsk',
    agent: 'agt',
    profile: 'prf',
    message: 'msg',
    decision: 'dec',
    finding: 'fnd',
    lease: 'lse',
    changeset: 'chg',
    audit: 'aud',
    checkpoint: 'ckp',
    correlation: 'cor',
};
/**
 * Mint a fresh identifier of one family.
 * @param kind - identifier family to mint.
 * @returns the prefixed UUID text carrying the family's brand.
 */
export function newId(kind) {
    return brandString(`${PREFIX[kind]}_${randomUUID()}`);
}
/**
 * Adopt existing identifier text as one family's brand.
 * @param kind - identifier family the text belongs to.
 * @param text - identifier text previously produced by {@link newId} or carried by a durable record.
 * @returns the same text with the family's brand.
 */
export function adoptId(kind, text) {
    if (!text.startsWith(`${PREFIX[kind]}_`)) {
        throw new Error(`ncag: ${JSON.stringify(text)} is not a ${kind} identifier (expected prefix "${PREFIX[kind]}_")`);
    }
    return brandString(text);
}
//# sourceMappingURL=ids.js.map