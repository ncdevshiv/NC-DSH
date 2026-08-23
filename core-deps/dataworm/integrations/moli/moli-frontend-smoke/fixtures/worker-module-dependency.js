export const base = 17;

export function describe(value) {
  return `module:${value}:${base + value}`;
}
