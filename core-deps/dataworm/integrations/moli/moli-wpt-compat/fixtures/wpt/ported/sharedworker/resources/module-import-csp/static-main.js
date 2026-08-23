import { value } from "./dependency.js";

onconnect = event => {
  event.ports[0].postMessage(["static", value]);
};
