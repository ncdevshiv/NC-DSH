const ECHO_BODY_HEX_URL = "/wpt/runtime/fetch/echo-body-hex";

function request_init_for_kind(kind) {
  const init = {
    method: "POST",
  };

  switch (kind) {
    case "string":
      init.body = "hello";
      return init;
    case "explicit-string":
      init.body = "hello";
      init.headers = [["Content-Type", "text/custom-worker"]];
      return init;
    case "params":
      const params = new URLSearchParams();
      params.append("alpha", "one");
      params.append("beta", "two");
      init.body = params;
      return init;
    case "explicit-params":
      init.body = new URLSearchParams([["alpha", "one"]]);
      init.headers = [["Content-Type", "application/custom-worker-form"]];
      return init;
    case "blob":
      init.body = new Blob([new Uint8Array([0x00, 0xff, 0x41, 0x80, 0x0a, 0x0d])], {
        type: "application/octet-stream",
      });
      return init;
    case "explicit-blob":
      init.body = new Blob([new Uint8Array([0x00, 0xff, 0x41])], {
        type: "application/octet-stream",
      });
      init.headers = [["Content-Type", "application/custom-worker-blob"]];
      return init;
    case "arraybuffer":
      init.body = new Uint8Array([0x01, 0x00, 0xff, 0x80]).buffer;
      return init;
    case "explicit-arraybuffer":
      init.body = new Uint8Array([0x01, 0x00, 0xff, 0x80]).buffer;
      init.headers = [["Content-Type", "application/custom-worker-bytes"]];
      return init;
    case "formdata-empty-mime": {
      const data = new FormData();
      data.append("empty-blob", new Blob(["blob-body"]));
      data.append("empty-file", new File(["file-body"], "empty.bin"));
      init.body = data;
      return init;
    }
    case "formdata-escaped-names": {
      const data = new FormData();
      data.append("alpha", "one");
      data.append("line\nname", "line\nvalue");
      data.append("carriage\rname", "carriage\rvalue");
      data.append('quote"name', 'quote"value');
      data.append("cafe-\u00e9", "\u503c-\ud83d\ude00");
      data.append('file"name', new File(["file-body"], 'report"name.txt', {
        type: "text/custom",
      }));
      data.append("unicode-file", new File(["unicode-file-body"], "resume-\u00e9-\ud83d\ude00.txt", {
        type: "text/unicode",
      }));
      init.body = data;
      return init;
    }
    case "formdata-boundary-collision": {
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
      init.body = data;
      return init;
    }
    case "explicit-formdata": {
      const data = new FormData();
      data.append("alpha", "one");
      init.body = data;
      init.headers = [["Content-Type", "text/custom-worker-formdata"]];
      return init;
    }
    case "typedarray":
      init.body = new Uint8Array([0x00, 0xff, 0x41, 0x80]);
      return init;
    case "dataview": {
      const buffer = new Uint8Array([0xee, 0x00, 0xff, 0x80, 0xdd]).buffer;
      init.body = new DataView(buffer, 1, 3);
      return init;
    }
    default:
      throw new Error("unknown worker fetch body case: " + kind);
  }
}

self.onmessage = async function (event) {
  const kind = event.data;
  try {
    const response = await fetch(ECHO_BODY_HEX_URL, request_init_for_kind(kind));
    const payload = JSON.parse(await response.text());
    postMessage({
      kind,
      status: response.status,
      body_hex: payload.body_hex,
      content_type: payload.content_type,
    });
  } catch (error) {
    postMessage({
      kind,
      error: String(error),
      name: error && error.name,
    });
  }
};
