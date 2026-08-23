export let counter = 1;

export function bump() {
  counter += 1;
  return counter;
}

export function setCounter(value) {
  counter = value;
}
