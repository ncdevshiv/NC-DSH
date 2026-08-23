import assert from 'node:assert/strict';
import test from 'node:test';

import { extractItems } from '../bench.mjs';

function candidate({ text, href = null, closestHref = null, childHref = null }) {
  const anchor = (value) => ({
    getAttribute(name) {
      return name === 'href' ? value : null;
    }
  });
  return {
    innerText: text,
    getAttribute(name) {
      return name === 'href' ? href : null;
    },
    closest(selector) {
      assert.equal(selector, 'a');
      return closestHref === null ? null : anchor(closestHref);
    },
    querySelector(selector) {
      assert.equal(selector, 'a');
      return childHref === null ? null : anchor(childHref);
    }
  };
}

test('item extraction snapshots the complete selector set in one page-realm job', async () => {
  const generations = {
    old: new Map([
      ['news', [
        candidate({ text: ' First story ', href: '/first' }),
        candidate({ text: 'First story', href: '/duplicate' }),
        candidate({ text: 'Nested story', closestHref: '/nested' })
      ]],
      ['sports', [
        candidate({ text: 'Sports story', childHref: '/sports' })
      ]]
    ]),
    replacement: new Map([
      ['news', []],
      ['sports', []]
    ])
  };
  let generation = 'old';
  let replacementScheduled = false;
  const selectorGenerations = [];
  const snapshots = [];
  const page = {
    locator() {
      assert.fail('extractItems must not create Locators');
    },
    async evaluate(project, selectors) {
      snapshots.push([...selectors]);
      const previousDocument = globalThis.document;
      globalThis.document = {
        baseURI: 'https://example.test/root/',
        querySelectorAll(selector) {
          selectorGenerations.push(`${generation}:${selector}`);
          if (!replacementScheduled) {
            replacementScheduled = true;
            queueMicrotask(() => {
              generation = 'replacement';
            });
          }
          return generations[generation].get(selector) ?? [];
        }
      };
      try {
        return project(selectors);
      } finally {
        if (previousDocument === undefined) {
          delete globalThis.document;
        } else {
          globalThis.document = previousDocument;
        }
      }
    }
  };

  const items = await extractItems(page, ['news', 'sports']);

  assert.deepEqual(snapshots, [['news', 'sports']]);
  assert.deepEqual(selectorGenerations, ['old:news', 'old:sports']);
  assert.equal(generation, 'replacement');
  assert.deepEqual(items, [
    { title: 'First story', link: 'https://example.test/first' },
    { title: 'Nested story', link: 'https://example.test/nested' },
    { title: 'Sports story', link: 'https://example.test/sports' }
  ]);
});
