import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { compileScript, parse } from "@vue/compiler-sfc";
import * as esbuild from "esbuild";

import {
  ADVANCED_PLATFORM_FAMILIES,
  buildCatalog,
  GALLERY_FAMILIES,
} from "./catalog.mjs";
import { htmlFor } from "./html.mjs";

const PROJECT_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const GENERATED_ROOT = path.join(PROJECT_ROOT, ".generated");
const DIST_ROOT = path.join(PROJECT_ROOT, "dist");
const fullCatalog = buildCatalog();

function optionValues(name) {
  const values = [];
  for (let index = 0; index < process.argv.length; index += 1) {
    if (process.argv[index] === name && process.argv[index + 1]) {
      values.push(process.argv[index + 1]);
      index += 1;
    }
  }
  return values;
}

const frameworkFilters = new Set(optionValues("--framework"));
const familyFilters = new Set(optionValues("--family"));
const catalog = fullCatalog.filter(
  (item) =>
    (frameworkFilters.size === 0 || frameworkFilters.has(item.framework)) &&
    (familyFilters.size === 0 || familyFilters.has(item.family)),
);

if (catalog.length === 0) {
  throw new Error("build selection contains no frontend smoke cases");
}

function relativeImport(fromFile, toFile) {
  let value = path.relative(path.dirname(fromFile), toFile).replaceAll(path.sep, "/");
  if (!value.startsWith(".")) {
    value = `./${value}`;
  }
  return value;
}

function entryExtension(framework) {
  return framework === "react" ? "tsx" : "ts";
}

function entrySource(item, file) {
  const harness = relativeImport(file, path.join(PROJECT_ROOT, "src/shared/harness.ts"));
  const types = relativeImport(file, path.join(PROJECT_ROOT, "src/shared/types.ts"));
  const sourceFamily = GALLERY_FAMILIES.includes(item.family)
    ? "gallery-cases"
    : ADVANCED_PLATFORM_FAMILIES.includes(item.family)
      ? "advanced-platform-cases"
      : item.family;
  const familyModule = relativeImport(
    file,
    path.join(PROJECT_ROOT, `src/${item.framework}/${sourceFamily}.${item.framework === "vue" ? "vue" : item.framework === "react" ? "tsx" : "ts"}`),
  );
  const meta = {
    id: item.id,
    framework: item.framework,
    family: item.family,
    complexity: item.complexity,
    title: item.title,
  };
  const spec = {
    variant: item.variant,
    seed: item.seed,
    size: item.size,
    slug: item.slug,
    title: item.title,
  };
  if (item.framework === "vue") {
    const vueMount = relativeImport(file, path.join(PROJECT_ROOT, "src/vue/mount.ts"));
    return `import component from ${JSON.stringify(familyModule)};
import { mountVue } from ${JSON.stringify(vueMount)};
import { beginCase, captureFrame, failCase } from ${JSON.stringify(harness)};
import type { CaseSpec, SmokeMeta } from ${JSON.stringify(types)};
const meta: SmokeMeta = ${JSON.stringify(meta)};
const spec: CaseSpec = ${JSON.stringify(spec)};
beginCase(meta);
void (async () => {
  await captureFrame(meta, "document");
  mountVue(component, meta, spec);
})().catch(failCase);
`;
  }
  return `import { mount } from ${JSON.stringify(familyModule)};
import { beginCase, captureFrame, failCase } from ${JSON.stringify(harness)};
import type { CaseSpec, SmokeMeta } from ${JSON.stringify(types)};
const meta: SmokeMeta = ${JSON.stringify(meta)};
const spec: CaseSpec = ${JSON.stringify(spec)};
beginCase(meta);
void (async () => {
  await captureFrame(meta, "document");
  await mount(meta, spec);
})().catch(failCase);
`;
}

function vuePlugin() {
  return {
    name: "moli-vue-sfc",
    setup(build) {
      build.onLoad({ filter: /\.vue$/ }, async ({ path: filename }) => {
        const source = await readFile(filename, "utf8");
        const id = createHash("sha256").update(filename).digest("hex").slice(0, 8);
        const parsed = parse(source, { filename });
        if (parsed.errors.length > 0) {
          throw new Error(`failed to parse ${filename}: ${parsed.errors.join("\n")}`);
        }
        if (!parsed.descriptor.scriptSetup) {
          throw new Error(`${filename} must use <script setup>`);
        }
        const result = compileScript(parsed.descriptor, {
          id,
          inlineTemplate: true,
        });
        return {
          contents: result.content,
          loader: parsed.descriptor.scriptSetup.lang === "ts" ? "ts" : "js",
          resolveDir: path.dirname(filename),
        };
      });
    },
  };
}

async function fixtureHash(root) {
  const digest = createHash("sha256");
  const files = [];
  async function collect(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      const filename = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        await collect(filename);
      } else if (entry.isFile()) {
        files.push(filename);
      }
    }
  }
  await collect(root);
  files.sort((left, right) => {
    const leftRelative = path.relative(root, left).replaceAll(path.sep, "/");
    const rightRelative = path.relative(root, right).replaceAll(path.sep, "/");
    return leftRelative < rightRelative ? -1 : leftRelative > rightRelative ? 1 : 0;
  });
  for (const filename of files) {
    const relative = path.relative(root, filename).replaceAll(path.sep, "/");
    digest.update(relative);
    digest.update("\0");
    digest.update(await readFile(filename));
    digest.update("\0");
  }
  return digest.digest("hex");
}

async function main() {
  await rm(GENERATED_ROOT, { recursive: true, force: true });
  await rm(DIST_ROOT, { recursive: true, force: true });
  await mkdir(GENERATED_ROOT, { recursive: true });
  await mkdir(DIST_ROOT, { recursive: true });
  await mkdir(path.join(DIST_ROOT, "data"), { recursive: true });
  await mkdir(path.join(DIST_ROOT, "support"), { recursive: true });
  await copyFile(
    path.join(PROJECT_ROOT, "fixtures", "web-platform-feed.json"),
    path.join(DIST_ROOT, "data", "web-platform-feed.json"),
  );
  await copyFile(
    path.join(PROJECT_ROOT, "fixtures", "boundary-frame.html"),
    path.join(DIST_ROOT, "support", "boundary-frame.html"),
  );
  for (const filename of [
    "worker-import-a.js",
    "worker-import-b.js",
    "worker-classic-import.js",
    "worker-module-dependency.js",
    "worker-module-entry.js",
    "shared-worker-multi.js",
  ]) {
    await copyFile(
      path.join(PROJECT_ROOT, "fixtures", filename),
      path.join(DIST_ROOT, "support", filename),
    );
  }

  const entryPoints = {};
  for (const item of catalog) {
    const file = path.join(
      GENERATED_ROOT,
      item.framework,
      item.family,
      `${item.slug}.${entryExtension(item.framework)}`,
    );
    await mkdir(path.dirname(file), { recursive: true });
    await writeFile(file, entrySource(item, file));
    entryPoints[`entries/${item.framework}/${item.family}/${item.slug}`] = file;
  }

  const result = await esbuild.build({
    absWorkingDir: PROJECT_ROOT,
    entryPoints,
    outdir: path.join(DIST_ROOT, "assets"),
    bundle: true,
    splitting: true,
    format: "esm",
    platform: "browser",
    target: ["chrome120"],
    entryNames: "[dir]/[name]",
    chunkNames: "chunks/[name]-[hash]",
    assetNames: "assets/[name]-[hash]",
    metafile: true,
    plugins: [vuePlugin()],
    define: {
      "process.env.NODE_ENV": '"production"',
      __VUE_OPTIONS_API__: "true",
      __VUE_PROD_DEVTOOLS__: "false",
      __VUE_PROD_HYDRATION_MISMATCH_DETAILS__: "false",
    },
    logLevel: "info",
  });

  for (const item of catalog) {
    const pageDir = path.join(DIST_ROOT, "cases", item.framework, item.family, item.slug);
    await mkdir(pageDir, { recursive: true });
    const entryUrl = `/assets/entries/${item.framework}/${item.family}/${item.slug}.js`;
    await writeFile(path.join(pageDir, "index.html"), htmlFor(item, entryUrl));
  }

  const fixturesSha256 = await fixtureHash(DIST_ROOT);
  const packageJson = JSON.parse(await readFile(path.join(PROJECT_ROOT, "package.json"), "utf8"));
  const manifest = {
    schemaVersion: 1,
    complete: catalog.length === fullCatalog.length,
    totalCatalogCases: fullCatalog.length,
    fixturesSha256,
    tools: {
      esbuild: packageJson.devDependencies.esbuild,
      react: packageJson.dependencies.react,
      vue: packageJson.dependencies.vue,
      angular: packageJson.dependencies["@angular/core"],
    },
    cases: catalog,
  };
  const manifestText = `${JSON.stringify(manifest, null, 2)}\n`;
  await writeFile(path.join(DIST_ROOT, "manifest.json"), manifestText);
  await writeFile(
    path.join(DIST_ROOT, "metafile.json"),
    `${JSON.stringify(result.metafile, null, 2)}\n`,
  );
  const hash = createHash("sha256").update(manifestText).digest("hex");
  console.log(
    JSON.stringify(
      {
        ok: true,
        cases: catalog.length,
        fixturesSha256,
        manifestSha256: hash,
        output: DIST_ROOT,
      },
      null,
      2,
    ),
  );
}

await main();
