self.__httpTlaPromisedOrder = [];
self.__httpTlaPromisedOrder.push("promise-start");
await Promise.resolve().then(function () {
  self.__httpTlaPromisedOrder.push("promise-after");
});
export const promised = self.__httpTlaPromisedOrder.join("|");
