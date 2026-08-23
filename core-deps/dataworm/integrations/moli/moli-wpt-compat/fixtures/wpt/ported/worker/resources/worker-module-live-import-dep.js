export let counter = 1;
export { counter as default };

export function bump() {
  counter += 1;
  return counter;
}

export function setCounter(value) {
  counter = value;
}
