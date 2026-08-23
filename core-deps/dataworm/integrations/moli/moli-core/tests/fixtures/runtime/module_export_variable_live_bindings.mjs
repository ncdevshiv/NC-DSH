export let counter = 0;
export var label = "init";

setTimeout(() => {
  counter = 2;
  label = "timeout";
}, 0);
