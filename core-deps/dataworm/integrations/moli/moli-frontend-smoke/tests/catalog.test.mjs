import assert from "node:assert/strict";
import test from "node:test";

import {
  ADVANCED_PLATFORM_FAMILIES,
  buildCatalog,
  FRAMEWORKS,
  GALLERY_FAMILIES,
} from "../scripts/catalog.mjs";

test("catalog includes browser integration and boundary families", () => {
  const catalog = buildCatalog();
  assert.equal(catalog.length, 1020);

  for (const framework of FRAMEWORKS) {
    for (const family of [
      "web-platform-integration",
      "browsing-context-boundaries",
      "network-storage-boundaries",
      ...ADVANCED_PLATFORM_FAMILIES,
    ]) {
      const cases = catalog.filter(
        (item) => item.framework === framework && item.family === family,
      );
      assert.equal(cases.length, 10);
      assert.deepEqual(new Set(cases.map((item) => item.complexity)), new Set(["complex"]));
    }
  }
});

test("catalog includes seventy gallery-inspired complex cases per framework", () => {
  const catalog = buildCatalog();

  for (const framework of FRAMEWORKS) {
    const cases = catalog.filter(
      (item) => item.framework === framework && GALLERY_FAMILIES.includes(item.family),
    );
    assert.equal(cases.length, 70);
    assert.deepEqual(new Set(cases.map((item) => item.complexity)), new Set(["complex"]));
    for (const family of GALLERY_FAMILIES) {
      assert.equal(
        cases.filter((item) => item.family === family).length,
        10,
        `${framework}/${family}`,
      );
    }
  }
});
