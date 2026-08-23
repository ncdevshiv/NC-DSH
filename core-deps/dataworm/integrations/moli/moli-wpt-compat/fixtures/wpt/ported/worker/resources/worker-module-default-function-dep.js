export default function namedDefaultFunction(value) {
  return "fn:" + value;
}

export const localFunctionResult = namedDefaultFunction("local");
