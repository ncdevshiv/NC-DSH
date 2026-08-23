async function collect(iterable) {
  const values = [];
  for await (const value of iterable) {
    values.push(value);
  }
  return values.join("|");
}

export default async function* namedDefaultAsyncGenerator(value) {
  yield "async-gen";
  yield value;
}

export const localAsyncGeneratorResult = await collect(namedDefaultAsyncGenerator("local"));
