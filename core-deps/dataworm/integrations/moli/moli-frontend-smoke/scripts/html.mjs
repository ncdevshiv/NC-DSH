const HTML_ESCAPES = Object.freeze({
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
});

export function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => HTML_ESCAPES[character]);
}

export function htmlFor(item, entryUrl) {
  const root =
    item.framework === "angular" ? "<smoke-root></smoke-root>" : '<div id="app"></div>';
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width,initial-scale=1">
    <link rel="icon" href="data:,">
    <title>${escapeHtml(item.title)}</title>
  </head>
  <body data-fixture-framework="${escapeHtml(item.framework)}" data-fixture-family="${escapeHtml(item.family)}">
    ${root}
    <noscript>JavaScript is required for this smoke fixture.</noscript>
    <script type="module" src="${escapeHtml(entryUrl)}"></script>
  </body>
</html>
`;
}
