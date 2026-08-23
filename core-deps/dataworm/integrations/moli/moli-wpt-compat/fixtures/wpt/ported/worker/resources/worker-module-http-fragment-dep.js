export let counter = 0;
export const moduleUrl = import.meta.url;

export function bump() {
  counter += 1;
  return counter;
}
