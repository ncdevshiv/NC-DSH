export default function* namedDefaultGenerator(value) {
  yield "gen";
  yield value;
}

export const localGeneratorResult = Array.from(namedDefaultGenerator("local")).join("|");
