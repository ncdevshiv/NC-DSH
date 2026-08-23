self.onmessage = async function (event) {
  const { kind, setUrl, checkUrl, credentials } = event.data;
  try {
    const response = await fetch(setUrl, credentials ? { credentials } : undefined);
    const beforeBody = await fetch(checkUrl, { credentials: "include" }).then((check) =>
      check.text()
    );
    const body = await response.text();
    const afterBody = await fetch(checkUrl, { credentials: "include" }).then((check) =>
      check.text()
    );

    postMessage({
      kind,
      status: response.status,
      beforeBody,
      body,
      afterBody,
    });
  } catch (error) {
    postMessage({
      kind,
      error: String(error),
      name: error && error.name,
    });
  }
};
