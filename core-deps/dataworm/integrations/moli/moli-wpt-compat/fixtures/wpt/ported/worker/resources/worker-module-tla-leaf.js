self.__httpTlaOrder = self.__httpTlaOrder || [];
self.__httpTlaOrder.push("leaf-start");
await Promise.resolve().then(function () {
  self.__httpTlaOrder.push("leaf-after");
});
export const leaf = self.__httpTlaOrder.join("|");
