export const config = {
  answer: 42,
  nested: [1, 2, 3],
  meta: { ok: "nested-ok", list: ["a", "b"] }
}, label = ["multi", "comma"].join("-");

export let state = {
  count: 1,
  items: ["x", "y"]
}, step = 1;

setTimeout(() => {
  state.count = 2;
  step = 2;
}, 0);
