export let counter = 1;
export { counter as default };

export function setCounter(value) {
  counter = value;
}
