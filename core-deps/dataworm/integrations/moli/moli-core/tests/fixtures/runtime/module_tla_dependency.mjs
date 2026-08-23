window.moduleTlaDependencyOrder.push("dep-start");
await new Promise((resolve) =>
  setTimeout(() => {
    window.moduleTlaDependencyOrder.push("dep-await-resolved");
    window.moduleTlaDependencyReady = true;
    resolve();
  }, 0)
);
window.moduleTlaDependencyOrder.push("dep-end");
export const greeting = "dep";
