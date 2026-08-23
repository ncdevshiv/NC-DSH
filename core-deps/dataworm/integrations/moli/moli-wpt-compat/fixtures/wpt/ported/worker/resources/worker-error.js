setTimeout(function () {
  throw new Error("worker-boom");
}, 20);
