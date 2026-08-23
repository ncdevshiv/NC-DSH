export const { answer, label: title } = {
  answer: 42,
  label: "destructure-ok"
};

export const [first, second] = [1, 2];

export let { count } = { count: 1 };
export let [left, ...tail] = [10, 20, 30];

setTimeout(() => {
  count = 2;
  left = 11;
  tail = [21, 31];
}, 0);
