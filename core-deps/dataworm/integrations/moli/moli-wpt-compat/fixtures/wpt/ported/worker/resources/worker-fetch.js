async function run() {
  const response = await fetch("./fetch-target.js");
  const objectResponse = await fetch({
    href: "./missing-target.js",
    toString() {
      return "./fetch-target.js";
    },
  });
  let missingInputError;
  try {
    await fetch();
  } catch (error) {
    missingInputError = error;
  }
  let badPortError;
  try {
    await fetch("http://example.test:25/blocked-port");
  } catch (error) {
    badPortError = error;
  }
  postMessage({
    ok: response.ok,
    status: response.status,
    url: response.url,
    text: (await response.text()).trim(),
    objectOk: objectResponse.ok,
    objectUrl: objectResponse.url,
    objectText: (await objectResponse.text()).trim(),
    missingInputName: missingInputError && missingInputError.name,
    missingInputMessage: String(missingInputError && missingInputError.message),
    badPortName: badPortError && badPortError.name,
    badPortMessage: String(badPortError && badPortError.message),
  });
}

run().catch(function (error) {
  postMessage({
    error: String(error),
    name: error && error.name,
  });
});
