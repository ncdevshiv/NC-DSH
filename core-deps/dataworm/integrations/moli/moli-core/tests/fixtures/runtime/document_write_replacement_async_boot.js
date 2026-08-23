document.open();
document.write(`<!doctype html>
<html>
  <head>
    <script>
      window.documentWriteReplacementInlineRuns =
        (window.documentWriteReplacementInlineRuns || 0) + 1;
      window.documentWriteReplacementAsyncOrder = [];
      document.addEventListener("DOMContentLoaded", () => {
        window.documentWriteReplacementAsyncOrder.push("dcl:" + document.readyState);
        window.documentWriteReplacementAsyncDclOrder =
          window.documentWriteReplacementAsyncOrder.join(",");
      });
      window.addEventListener("load", () => {
        window.documentWriteReplacementAsyncOrder.push("window-load:" + document.readyState);
        window.documentWriteReplacementAsyncFinalOrder =
          window.documentWriteReplacementAsyncOrder.join(",");
      });
    </script>
    <script
      async
      src="/assets/document_write_replacement_async.js"
      onload="window.documentWriteReplacementAsyncOrder.push('load:' + document.readyState); window.documentWriteReplacementAsyncLoadOrder = window.documentWriteReplacementAsyncOrder.join(',');"
    ></script>
  </head>
  <body>
    <main id="replacement">replacement</main>
  </body>
</html>`);
document.close();
