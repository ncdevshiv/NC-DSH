from __future__ import annotations

from .dom import DOM_CASES, response_for_dom_path
from .io import IO_CASES, response_for_io_path
from .modules import MODULE_CASES, response_for_module_path


WEBAPI_CASES = MODULE_CASES + DOM_CASES + IO_CASES


def response_for_webapi_path(path: str) -> tuple[str, bytes, float] | None:
    for resolver in (response_for_module_path, response_for_dom_path, response_for_io_path):
        response = resolver(path)
        if response is not None:
            return response
    return None
