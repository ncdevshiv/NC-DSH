self.onmessage = function (event) {
  const data = event.data || {};
  const kind = data.kind;
  const xhr = new XMLHttpRequest();
  const url = kind === "formdata-binary" || kind === "formdata-unicode"
    ? "/wpt/runtime/xhr/echo-body-hex"
    : "/wpt/runtime/xhr/echo-body";

  xhr.onload = function () {
    postMessage({
      kind,
      status: xhr.status,
      response: xhr.response,
    });
    close();
  };
  xhr.onerror = function () {
    postMessage({
      kind,
      error: "XMLHttpRequest failed",
      status: xhr.status,
    });
    close();
  };

  xhr.open("POST", url);
  xhr.responseType = "json";

  if (kind === "string") {
    xhr.send("plain-body");
  } else if (kind === "params") {
    const params = new URLSearchParams();
    params.append("alpha", "one");
    params.append("space", "two words");
    xhr.send(params);
  } else if (kind === "explicit-params") {
    const params = new URLSearchParams();
    params.append("alpha", "one");
    xhr.setRequestHeader("Content-Type", "text/plain;charset=utf-8");
    xhr.send(params);
  } else if (kind === "blob") {
    xhr.send(new Blob(["blob-body"], { type: "text/plain;charset=utf-8" }));
  } else if (kind === "buffer") {
    xhr.send(new Uint8Array([65, 66, 67]));
  } else if (kind === "explicit-buffer") {
    xhr.setRequestHeader("Content-Type", "application/custom-worker-bytes");
    xhr.send(new Uint8Array([65, 66, 67]));
  } else if (kind === "formdata-string") {
    const data = new FormData();
    data.append("alpha", "one");
    data.append("line\nname", "line\nvalue");
    data.append('quote"name', 'quote"value');
    xhr.send(data);
  } else if (kind === "formdata-file-empty-mime") {
    const data = new FormData();
    data.append("before", "text");
    data.append("blob", new Blob(["blob-body"], { type: "text/plain" }));
    data.append('file"name', new File(["file-body"], 'report"name.txt', {
      type: "text/custom",
    }));
    data.append("empty-blob", new Blob(["empty-blob-body"]));
    data.append("empty-file", new File(["empty-file-body"], "empty.bin"));
    xhr.send(data);
  } else if (kind === "formdata-boundary-collision") {
    const probes = [
      "----MoliFormDataBoundary0000000000000000",
      "----MoliFormDataBoundary0123456789abcdef",
      "----MoliFormDataBoundaryffffffffffffffff",
    ];
    const data = new FormData();
    data.append(`field ${probes[0]}`, `text value ${probes[1]}`);
    data.append("file", new File([`file bytes ${probes[2]}`], `report ${probes[0]}.txt`, {
      type: "application/x-moliformdataboundary",
    }));
    data.append("tail", probes.join("|"));
    xhr.send(data);
  } else if (kind === "explicit-formdata") {
    const data = new FormData();
    data.append("alpha", "one");
    xhr.setRequestHeader("Content-Type", "text/custom-worker-formdata");
    xhr.send(data);
  } else if (kind === "formdata-binary") {
    const data = new FormData();
    data.append("prefix", "text");
    data.append("binary", new Blob([
      new Uint8Array([0x00, 0xff, 0x41, 0x80, 0x0a, 0x0d]),
    ], { type: "application/octet-stream" }));
    xhr.send(data);
  } else if (kind === "formdata-unicode") {
    const data = new FormData();
    data.append("carriage\rname", "carriage\rvalue");
    data.append("cafe-é", "值-😀");
    data.append("unicode-file", new File(["unicode-file-body"], "resume-é-😀.txt", {
      type: "text/unicode",
    }));
    xhr.send(data);
  } else {
    postMessage({
      kind,
      error: "unknown body kind: " + kind,
    });
    close();
  }
};
