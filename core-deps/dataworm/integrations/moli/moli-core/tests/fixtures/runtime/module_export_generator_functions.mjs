export function* values() {
  yield "one";
  yield "two";
}

export async function* asyncValues() {
  yield "async-one";
  await Promise.resolve();
  yield "async-two";
}

export default function* () {
  yield "default-one";
  yield "default-two";
}
