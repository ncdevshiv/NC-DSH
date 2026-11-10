/**
 * Schema-driven field descriptors for the auto-generated settings page.
 *
 * The Host already publishes each plugin's editable form as a serialized
 * schemastery envelope, and no client consumed it: every configuration page
 * in the product was written by hand against fields the wire had been serving
 * all along. This module is the reader for that envelope. It classifies each
 * field into the smallest control that can represent it, and hands the page a
 * flat, path-addressed tree rather than a schema, so the renderer never
 * reasons about schemastery itself.
 *
 * Classification is total. Every field ends up as exactly one descriptor, so
 * a schema shape this module has never seen renders as a validated JSON
 * editor instead of disappearing — a page that silently drops fields is worse
 * than one that offers them all.
 */

import type Schema from '@deepseek-ai/schemastery'

/** One choice a select field offers, carrying the value it stores verbatim. */
export interface AutoOption {
  /** Value written when the option is chosen. */
  value: string
  /** Display text for the option. */
  label: string
}

/** What every descriptor carries regardless of its control. */
export interface AutoFieldBase {
  /** Path from the section root addressing this field's value. */
  path: readonly string[]
  /** Display label: the schema's own description when it declares one, else the key read as words. */
  label: string
  /** Schema-declared explanation, or the empty string when the schema declares none. */
  hint: string
  /**
   * Whether the schema marks this field required. A required field re-inherits
   * the composition layer rather than clearing, so the page offers no reset.
   */
  required: boolean
}

/** The scalar editors a repeated row can use. */
export type AutoScalarKind = 'text' | 'number'

/**
 * One editable field, classified for the page to render.
 *
 * `rows` and `pairs` are the homogeneous collections: an array of scalars
 * becomes add/remove text or number rows, and a dictionary of scalars becomes
 * key/value rows. `json` is the general form — any other structure edits as
 * JSON validated against its own subtree schema before the write is staged.
 */
export type AutoField =
  | (AutoFieldBase & { kind: 'toggle' })
  | (AutoFieldBase & { kind: 'text' })
  | (AutoFieldBase & { kind: 'number' })
  | (AutoFieldBase & { kind: 'select'; options: readonly AutoOption[] })
  | (AutoFieldBase & { kind: 'rows'; item: AutoScalarKind })
  | (AutoFieldBase & { kind: 'pairs' })
  | (AutoFieldBase & { kind: 'json' })
  | (AutoFieldBase & { kind: 'group'; children: readonly AutoField[] })

/** A schema description is only usable as prose when it is one string. */
function description(schema: Schema): string | undefined {
  const declared = schema.meta.description
  return typeof declared === 'string' && declared !== '' ? declared : undefined
}

/**
 * Read a configuration key as display words.
 *
 * Plugin Config fields are code identifiers, and the Host does not always
 * attach prose to them. `baseURL` becomes "Base URL" and `maxImagesPerRequest`
 * becomes "Max images per request" by splitting on the camel boundary while
 * preserving tokens that are entirely capitals, so an acronym survives. This
 * is a mechanical reading of the key, not authored copy: a schema that
 * declares a description always shows that description instead.
 * @param key - configuration field name.
 * @returns the key as title-cased words.
 */
export function humanizeKey(key: string): string {
  const words = key
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')
    .split(/[\s_-]+/)
    .filter(word => word !== '')
  if (words.length === 0) return key
  return words
    .map((word, index) => {
      if (index > 0 && word.length > 1 && /^[A-Z0-9]+$/.test(word)) return word
      const lower = word.toLowerCase()
      return index === 0 ? lower.charAt(0).toUpperCase() + lower.slice(1) : lower
    })
    .join(' ')
}

/** Whether a field's values can be staged through a single-line text or number control. */
function scalarOf(schema: Schema): AutoScalarKind | undefined {
  if (schema.type === 'string') return 'text'
  if (schema.type === 'number') return 'number'
  return undefined
}

/** Classify a leaf field, or an object into the group holding its own children. */
function classify(schema: Schema, path: readonly string[], label: string): AutoField {
  const base: AutoFieldBase = {
    path,
    label: description(schema) ?? label,
    hint: description(schema) ?? '',
    required: schema.meta.required === true,
  }
  switch (schema.type) {
    case 'object': {
      const children = childrenOf(schema, path)
      // An object with nothing editable inside is not a control at all.
      return children.length === 0 ? jsonField(base) : { ...base, kind: 'group', children }
    }
    case 'boolean':
      return { ...base, kind: 'toggle' }
    case 'number':
      return { ...base, kind: 'number' }
    case 'string':
      return { ...base, kind: 'text' }
    case 'const':
      return { ...base, kind: 'select', options: [{ value: String(schema.value), label: String(schema.value) }] }
    case 'union': {
      const branches = schema.list ?? []
      // A union of literals is the shape schemastery gives every closed string
      // set, so it is the common case and the only one worth special-casing.
      if (branches.length > 0 && branches.every(branch => branch.type === 'const')) {
        return {
          ...base,
          kind: 'select',
          options: branches.map(branch => ({ value: String(branch.value), label: String(branch.value) })),
        }
      }
      if (branches.length === 1) return classify(branches[0] as Schema, path, label)
      // A union of objects is how a discriminated Config is written — the MCP
      // transport choice and the browser launch/attach choice are both this.
      // Its variants share the discriminator, and the Host serves a form only
      // when every variant carries editable fields, so their fields merge into
      // one flat set: a path the active variant lacks simply reads absent.
      if (branches.length > 0 && branches.every(branch => branch.type === 'object')) {
        const children = mergeVariants(branches as Schema[], path)
        return children.length === 0 ? jsonField(base) : { ...base, kind: 'group', children }
      }
      return jsonField(base)
    }
    case 'dict':
      return scalarOf(schema.inner as Schema) === undefined ? jsonField(base) : { ...base, kind: 'pairs' }
    case 'array':
      return scalarOf(schema.inner as Schema) === undefined
        ? jsonField(base)
        : { ...base, kind: 'rows', item: scalarOf(schema.inner as Schema) as AutoScalarKind }
    default:
      return jsonField(base)
  }
}

/** The general editor: any structure this module has no dedicated control for. */
function jsonField(base: AutoFieldBase): AutoField {
  return { ...base, kind: 'json' }
}

/** The editable children of an object field, each classified against its own key. */
function childrenOf(schema: Schema, path: readonly string[]): AutoField[] {
  return Object.entries(schema.dict ?? {}).flatMap(([key, child]) => {
    if (child.meta.hidden === true) return []
    return [classify(child as Schema, [...path, key], humanizeKey(key))]
  })
}

/**
 * Merge a discriminated union's variants into one field list.
 *
 * Keys shared by every variant keep the first classification, so a field typed
 * the same way across transports is described once. A key present in only some
 * variants is kept too: it is still editable for the variants that declare it,
 * and the page reads it as absent elsewhere rather than hiding a real setting.
 */
function mergeVariants(branches: readonly Schema[], path: readonly string[]): AutoField[] {
  const merged = new Map<string, AutoField>()
  for (const branch of branches) {
    for (const field of childrenOf(branch, path)) {
      const key = field.path.join('/')
      if (!merged.has(key)) merged.set(key, field)
    }
  }
  return [...merged.values()]
}

/**
 * Read a serialized form envelope as the fields a settings page renders.
 *
 * The Host publishes an object's editable subtree, so a form schema is
 * normally an object and its children are the fields. A form whose root is
 * not an object still renders — as one JSON editor — rather than rendering
 * nothing.
 * @param schema - rehydrated form schema from a namespace's wire envelope.
 * @returns the fields to render, in schema order.
 */
export function autoFields(schema: Schema): AutoField[] {
  if (schema.type !== 'object') return [classify(schema, [], schema.type)]
  return childrenOf(schema, [])
}
