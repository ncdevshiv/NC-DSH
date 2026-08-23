export default async function namedDefaultAsyncFunction(value) {
  return "async-fn:" + value;
}

export const localAsyncFunctionResult = await namedDefaultAsyncFunction("local");
