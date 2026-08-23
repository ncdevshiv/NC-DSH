self.__httpTlaReexportOrder = self.__httpTlaReexportOrder || [];
self.__httpTlaReexportOrder.push("reexport-source-start");
await Promise.resolve().then(function () {
  self.__httpTlaReexportOrder.push("reexport-source-after");
});
export const source = self.__httpTlaReexportOrder.join("|");
