// Keep evaluation pending so the test observes load-blocking semantics without a clock race.
await new Promise(() => {});
window.dynamicAsyncModuleReady = true;
