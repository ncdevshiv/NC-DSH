from __future__ import annotations

from dataclasses import dataclass

from .synthetic_case_groups.basic import BASIC_CASES, response_for_basic_path
from .synthetic_case_groups.webapi import WEBAPI_CASES, response_for_webapi_path


SYNTHETIC_CASES = BASIC_CASES + WEBAPI_CASES


@dataclass(frozen=True)
class SyntheticResponse:
    content_type: str
    body: bytes
    delay_seconds: float = 0.0


def html_document(body: str, script: str = "") -> bytes:
    return (
        "<!doctype html><meta charset=utf-8>"
        "<title>moli benchmark</title>"
        f"<body>{body}<script>{script}</script></body>"
    ).encode("utf-8")


def response_for_path(path: str) -> SyntheticResponse | None:
    response = response_for_basic_path(path)
    if response is not None:
        return SyntheticResponse(*response)
    response = response_for_webapi_path(path)
    if response is not None:
        return SyntheticResponse(*response)
    return None
