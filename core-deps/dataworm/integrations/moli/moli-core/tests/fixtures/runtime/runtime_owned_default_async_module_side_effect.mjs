window.runtimeOwnedAsyncModuleOrder.push("module");
queueMicrotask(() => window.runtimeOwnedAsyncModuleOrder.push("module-microtask"));
export const ok = 1;
