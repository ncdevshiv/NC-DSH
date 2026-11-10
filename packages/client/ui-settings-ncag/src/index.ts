/**
 * NC-AG settings page, node half. The empty apply exists so the plugin appears
 * in the host cordis.yml / Loader; the browser half owns the page through
 * exports["./client"], discovered from the package.json dsh.client declaration.
 * The configuration namespace the page edits is `ncag-settings`, owned by
 * `@deepseek-ai/dsh-ncag-settings`; the monitor reads the platform over the
 * `ncag` Remote namespace owned by `@deepseek-ai/dsh-api-ncag-controller`.
 */

/** Host plugin body — no host-side behavior for this surface plugin. */
export function apply(): void {}
