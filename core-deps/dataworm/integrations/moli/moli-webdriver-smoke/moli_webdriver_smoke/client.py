from __future__ import annotations

import json
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Any


CLASSIC_ELEMENT_REFERENCE_KEY = "element-6066-11e4-a52e-4f735466cecf"
CLASSIC_SHADOW_ROOT_REFERENCE_KEY = "shadow-6066-11e4-a52e-4f735466cecf"


@dataclass(frozen=True)
class WebDriverResponse:
    status: int
    body: dict[str, Any]


class WebDriverHttpError(RuntimeError):
    def __init__(self, response: WebDriverResponse) -> None:
        super().__init__(f"WebDriver HTTP {response.status}: {response.body!r}")
        self.response = response


class ClassicClient:
    def __init__(self, endpoint: str) -> None:
        self.endpoint = endpoint.rstrip("/")

    def get(self, path: str, *, expected_status: int = 200) -> dict[str, Any]:
        return self.request("GET", path, expected_status=expected_status).body

    def post(self, path: str, body: dict[str, Any] | None = None, *, expected_status: int = 200) -> dict[str, Any]:
        return self.request("POST", path, body, expected_status=expected_status).body

    def delete(self, path: str, *, expected_status: int = 200) -> dict[str, Any]:
        return self.request("DELETE", path, expected_status=expected_status).body

    def request(
        self,
        method: str,
        path: str,
        body: dict[str, Any] | None = None,
        *,
        expected_status: int = 200,
    ) -> WebDriverResponse:
        url = self.endpoint + path
        data = None
        headers = {"Accept": "application/json"}
        if body is not None:
            data = json.dumps(body).encode("utf-8")
            headers["Content-Type"] = "application/json"
        request = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=10) as response:
                response_body = _read_json_response(response.read())
                result = WebDriverResponse(response.status, response_body)
        except urllib.error.HTTPError as error:
            response_body = _read_json_response(error.read())
            result = WebDriverResponse(error.code, response_body)
        if result.status != expected_status:
            raise WebDriverHttpError(result)
        return result


def _read_json_response(payload: bytes) -> dict[str, Any]:
    text = payload.decode("utf-8", errors="replace")
    parsed = json.loads(text)
    if not isinstance(parsed, dict):
        raise RuntimeError(f"expected JSON object response, got {parsed!r}")
    return parsed


def classic_value(response: dict[str, Any]) -> Any:
    if "value" not in response:
        raise RuntimeError(f"missing WebDriver value envelope: {response!r}")
    return response["value"]


def classic_element_id(reference: dict[str, Any]) -> str:
    value = classic_value(reference)
    if not isinstance(value, dict):
        raise RuntimeError(f"expected element reference object, got {reference!r}")
    element_id = value.get(CLASSIC_ELEMENT_REFERENCE_KEY)
    if not isinstance(element_id, str) or not element_id:
        raise RuntimeError(f"missing element reference id: {reference!r}")
    return element_id


def classic_shadow_root_id(reference: dict[str, Any]) -> str:
    value = classic_value(reference)
    if not isinstance(value, dict):
        raise RuntimeError(f"expected shadow root reference object, got {reference!r}")
    shadow_id = value.get(CLASSIC_SHADOW_ROOT_REFERENCE_KEY)
    if not isinstance(shadow_id, str) or not shadow_id:
        raise RuntimeError(f"missing shadow root reference id: {reference!r}")
    return shadow_id
