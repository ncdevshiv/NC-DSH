import { buildCatalog, FAMILIES, FRAMEWORKS } from "./catalog.mjs";

const cases = buildCatalog();
const errors = [];
const ids = new Set();
const paths = new Set();

for (const item of cases) {
  if (ids.has(item.id)) {
    errors.push(`duplicate id: ${item.id}`);
  }
  if (paths.has(item.path)) {
    errors.push(`duplicate path: ${item.path}`);
  }
  ids.add(item.id);
  paths.add(item.path);
}

if (cases.length < 1020) {
  errors.push(`expected at least 1020 cases, got ${cases.length}`);
}

for (const framework of FRAMEWORKS) {
  const selected = cases.filter((item) => item.framework === framework);
  if (selected.length < 340) {
    errors.push(`${framework}: expected at least 340 cases, got ${selected.length}`);
  }
  const counts = Object.groupBy(selected, (item) => item.complexity);
  for (const [complexity, expected] of Object.entries({
    simple: 40,
    medium: 40,
    complex: 260,
  })) {
    const actual = counts[complexity]?.length ?? 0;
    if (actual !== expected) {
      errors.push(`${framework}/${complexity}: expected ${expected}, got ${actual}`);
    }
  }
  for (const family of FAMILIES) {
    const actual = selected.filter((item) => item.family === family).length;
    if (actual !== 10) {
      errors.push(`${framework}/${family}: expected 10, got ${actual}`);
    }
  }
}

if (errors.length > 0) {
  for (const error of errors) {
    console.error(error);
  }
  process.exitCode = 1;
} else {
  console.log(
    JSON.stringify(
      {
        ok: true,
        count: cases.length,
        frameworks: Object.fromEntries(
          FRAMEWORKS.map((framework) => [
            framework,
            cases.filter((item) => item.framework === framework).length,
          ]),
        ),
      },
      null,
      2,
    ),
  );
}
