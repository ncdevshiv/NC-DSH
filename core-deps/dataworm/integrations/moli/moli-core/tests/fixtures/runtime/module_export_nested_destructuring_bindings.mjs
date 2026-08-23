export const {
  outer: { answer },
  labels: [primary, ...others],
  meta: { title = "nested-default" }
} = {
  outer: { answer: 42 },
  labels: ["lead", "tail-a", "tail-b"],
  meta: {}
};

export let {
  stats: { count }
} = {
  stats: { count: 1 }
};

export let [
  first,
  { deep },
  ...rest
] = [10, { deep: 20 }, 30, 40];

setTimeout(() => {
  count = 2;
  first = 11;
  deep = 21;
  rest = [31, 41];
}, 0);
