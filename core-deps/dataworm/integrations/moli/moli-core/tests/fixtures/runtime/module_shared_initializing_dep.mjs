window.moduleSharedInitializingOrder.push("dep-start");
await new Promise((resolve) => {
  setTimeout(() => {
    window.moduleSharedInitializingOrder.push("dep-end");
    resolve();
  }, 0);
});

export const depValue = "ready";
