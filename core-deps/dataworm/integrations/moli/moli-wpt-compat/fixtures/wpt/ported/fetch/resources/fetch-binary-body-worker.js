async function run() {
  const bytes = [0x00, 0xff, 0x41, 0x80];
  const response = await fetch("/wpt/runtime/fetch/echo-body-hex", {
    method: "POST",
    body: new Uint8Array(bytes),
  });
  const payload = JSON.parse(await response.text());
  postMessage({
    status: response.status,
    body_hex: payload.body_hex,
  });
}

run().catch(function (error) {
  postMessage({
    error: String(error),
    name: error && error.name,
  });
});
