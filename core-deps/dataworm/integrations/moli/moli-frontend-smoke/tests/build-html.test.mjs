import assert from "node:assert/strict";
import test from "node:test";

import { escapeHtml, htmlFor } from "../scripts/html.mjs";

test("escapeHtml encodes text and attribute metacharacters", () => {
  assert.equal(
    escapeHtml(`Tom & <friends> say "hello" and 'goodbye'`),
    "Tom &amp; &lt;friends&gt; say &quot;hello&quot; and &#39;goodbye&#39;",
  );
});

test("htmlFor escapes every catalog-derived or generated interpolation", () => {
  const html = htmlFor(
    {
      framework: `rea"ct&<`,
      family: `forms"><script>bad()</script>&'`,
      title: `A </title><script>bad()</script> & "'`,
    },
    `./entry.mjs?value=<bad>&quote="'`,
  );

  assert.match(
    html,
    /<title>A &lt;\/title&gt;&lt;script&gt;bad\(\)&lt;\/script&gt; &amp; &quot;&#39;<\/title>/,
  );
  assert.match(html, /data-fixture-framework="rea&quot;ct&amp;&lt;"/);
  assert.match(
    html,
    /data-fixture-family="forms&quot;&gt;&lt;script&gt;bad\(\)&lt;\/script&gt;&amp;&#39;"/,
  );
  assert.match(
    html,
    /src="\.\/entry\.mjs\?value=&lt;bad&gt;&amp;quote=&quot;&#39;"/,
  );
  assert.doesNotMatch(html, /<script>bad\(\)<\/script>/);
});

test("htmlFor keeps the Angular host element", () => {
  assert.match(
    htmlFor(
      { framework: "angular", family: "components", title: "Angular fixture" },
      "./entry.mjs",
    ),
    /<smoke-root><\/smoke-root>/,
  );
});
