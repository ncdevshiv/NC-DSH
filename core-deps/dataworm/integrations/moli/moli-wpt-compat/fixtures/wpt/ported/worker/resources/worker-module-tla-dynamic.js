self.__httpTlaDynamicOrder = [];
self.__httpTlaDynamicOrder.push("dynamic-start");
await Promise.resolve().then(function () {
  self.__httpTlaDynamicOrder.push("dynamic-after");
});
export const dynamic = self.__httpTlaDynamicOrder.join("|");
