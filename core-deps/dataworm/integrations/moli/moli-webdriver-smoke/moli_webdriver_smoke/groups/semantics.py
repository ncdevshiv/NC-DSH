from __future__ import annotations

import asyncio
from contextlib import contextmanager
from dataclasses import dataclass
from typing import Any, Callable, Iterator

from selenium import webdriver
from selenium.common.exceptions import (
    InvalidSelectorException,
    NoSuchFrameException,
    NoSuchWindowException,
    SessionNotCreatedException,
    StaleElementReferenceException,
)
from selenium.webdriver.common.by import By
from selenium.webdriver.support import expected_conditions as EC
from selenium.webdriver.support.ui import WebDriverWait

from ..assertions import SmokeError, assert_equal, assert_true, record_contract
from ..config import WebDriverTarget
from ..selenium_options import create_selenium_options


SemanticScenario = Callable[[WebDriverTarget, str], dict[str, Any]]


@dataclass(frozen=True)
class SemanticContract:
    name: str
    contract: str
    source: str
    commands: list[str]
    scenario: SemanticScenario


async def run_semantics_group(
    target: WebDriverTarget,
    fixture: str,
    results: list[dict[str, Any]],
    _continue_on_failure: bool = False,
) -> None:
    await asyncio.to_thread(_run_semantics_group_sync, target, fixture, results)


def _run_semantics_group_sync(
    target: WebDriverTarget,
    fixture: str,
    results: list[dict[str, Any]],
) -> None:
    for item in _semantic_contracts():
        try:
            observed = item.scenario(target, fixture)
        except Exception as error:
            results.append(
                {
                    "name": item.name,
                    "group": "semantics",
                    "ok": False,
                    "contract": item.contract,
                    "source": item.source,
                    "commands": item.commands,
                    "errorType": type(error).__name__,
                    "error": str(error),
                }
            )
        else:
            record_contract(
                results,
                item.name,
                contract=item.contract,
                source=item.source,
                commands=item.commands,
                observed=observed,
            )


def _semantic_contracts() -> tuple[SemanticContract, ...]:
    return (
        SemanticContract(
            "webdriver_contract_capability_matching",
            "A matching browserName creates a session with the expected product capability, while an impossible browserName is rejected with session not created.",
            "W3C WebDriver session creation and Chromium behavior",
            ["POST /session", "DELETE /session/{sessionId}"],
            _capability_matching,
        ),
        SemanticContract(
            "webdriver_contract_same_document_history_identity",
            "Back and forward across pushState entries preserve the active document, JavaScript realm, and WebElement identity.",
            "HTML History, W3C WebDriver, and Chromium behavior",
            ["GET /url", "POST /execute/sync", "POST /back", "POST /forward"],
            _same_document_history_identity,
        ),
        SemanticContract(
            "webdriver_contract_cross_document_stale_element",
            "A WebElement owned by the previous active document becomes stale after a cross-document navigation.",
            "W3C WebDriver stale element semantics and Chromium behavior",
            ["GET /url x2", "GET /element/{elementId}/name"],
            _cross_document_stale_element,
        ),
        SemanticContract(
            "webdriver_contract_top_level_storage_namespaces",
            "Same-origin top-level windows share localStorage, isolate sessionStorage, and preserve each namespace across reload.",
            "HTML Web Storage and Chromium behavior",
            ["POST /window/new", "POST /window", "POST /refresh", "POST /execute/sync"],
            _top_level_storage_namespaces,
        ),
        SemanticContract(
            "webdriver_contract_popup_storage_snapshot",
            "A popup receives a creation-time copy of the opener sessionStorage and later mutations do not flow back to the opener.",
            "HTML Web Storage and Chromium behavior",
            ["POST /execute/sync window.open", "GET /window/handles", "POST /window"],
            _popup_storage_snapshot,
        ),
        SemanticContract(
            "webdriver_contract_nested_frame_routing_and_recovery",
            "Frame selection by id, name, and element routes commands to the right document; after removal the old frame is unavailable and a sibling remains usable.",
            "W3C WebDriver frame switching and Chromium behavior",
            ["POST /frame", "POST /frame/parent", "POST /execute/sync"],
            _nested_frame_routing_and_recovery,
        ),
        SemanticContract(
            "webdriver_contract_shadow_frame_window_named_access",
            "A connected iframe in a shadow tree has a contentWindow but contributes neither its name nor id to Window named properties.",
            "HTML Window named access and Chromium behavior",
            ["GET /url", "POST /execute/sync"],
            _shadow_frame_window_named_access,
        ),
        SemanticContract(
            "webdriver_contract_dialog_return_values",
            "Timer-triggered confirm and prompt dialogs synchronously resume script with the accepted, dismissed, entered, or null result.",
            "HTML dialogs, W3C WebDriver, and Chromium behavior",
            ["POST /execute/sync", "GET/POST /alert/*"],
            _dialog_return_values,
        ),
        SemanticContract(
            "webdriver_contract_standard_error_types",
            "Invalid selectors, missing frames, missing windows, and stale elements surface their matching W3C WebDriver error classes without corrupting the session.",
            "W3C WebDriver errors and Chromium behavior",
            ["POST /elements", "POST /frame", "POST /window", "GET /element/{elementId}/name"],
            _standard_error_types,
        ),
    )


@contextmanager
def _driver(target: WebDriverTarget) -> Iterator[webdriver.Remote]:
    options = create_selenium_options(target)
    driver = webdriver.Remote(command_executor=target.endpoint, options=options)
    try:
        yield driver
    finally:
        driver.quit()


def _wait_for_url(driver: webdriver.Remote, expected: str) -> None:
    WebDriverWait(driver, 5, poll_frequency=0.05).until(
        lambda current: current.current_url == expected
    )


def _expect_exception(error_type: type[Exception], operation: Callable[[], Any]) -> str:
    try:
        operation()
    except error_type as error:
        return type(error).__name__
    except Exception as error:
        raise SmokeError(
            f"expected {error_type.__name__}, got {type(error).__name__}: {error}"
        ) from error
    raise SmokeError(f"expected {error_type.__name__}, command succeeded")


def _capability_matching(target: WebDriverTarget, _fixture: str) -> dict[str, Any]:
    with _driver(target) as driver:
        returned_name = driver.capabilities.get("browserName")
        assert_equal(
            returned_name,
            target.browser_name,
            "returned browserName capability",
        )
        session_id = driver.session_id

    impossible_name = "moli-smoke-impossible-browser"
    mismatch_options = create_selenium_options(
        target,
        requested_browser_name=impossible_name,
    )
    mismatch_driver: webdriver.Remote | None = None
    try:
        mismatch_driver = webdriver.Remote(
            command_executor=target.endpoint,
            options=mismatch_options,
        )
    except SessionNotCreatedException as error:
        mismatch_error = type(error).__name__
    else:
        returned = mismatch_driver.capabilities.get("browserName")
        raise SmokeError(
            "impossible browserName unexpectedly created a session: "
            f"requested={impossible_name!r}, returned={returned!r}"
        )
    finally:
        if mismatch_driver is not None:
            mismatch_driver.quit()

    return {
        "sessionIdPresent": bool(session_id),
        "returnedBrowserName": returned_name,
        "mismatchBrowserName": impossible_name,
        "mismatchError": mismatch_error,
    }


def _same_document_history_identity(
    target: WebDriverTarget,
    fixture: str,
) -> dict[str, Any]:
    with _driver(target) as driver:
        base_url = f"{fixture}/webdriver/basic#base"
        second_url = f"{fixture}/webdriver/basic#two"
        third_url = f"{fixture}/webdriver/basic#three"
        driver.get(base_url)
        element = driver.find_element(By.ID, "main")
        driver.execute_script(
            """
            window.__webdriverRealmMarker = {value: 17};
            arguments[0].__webdriverNodeMarker = 23;
            history.pushState({step: 1}, '', '#one');
            history.replaceState({step: 2}, '', '#two');
            history.pushState({step: 3}, '', '#three');
            """,
            element,
        )
        assert_equal(driver.current_url, third_url, "history URL after push/replace")

        driver.back()
        _wait_for_url(driver, second_url)
        after_back = driver.execute_script(
            "return {realm: window.__webdriverRealmMarker.value, node: arguments[0].__webdriverNodeMarker};",
            element,
        )
        assert_equal(after_back, {"realm": 17, "node": 23}, "same-document identity after back")

        driver.forward()
        _wait_for_url(driver, third_url)
        after_forward = driver.execute_script(
            "return {realm: window.__webdriverRealmMarker.value, node: arguments[0].__webdriverNodeMarker};",
            element,
        )
        assert_equal(
            after_forward,
            {"realm": 17, "node": 23},
            "same-document identity after forward",
        )
        return {
            "urls": [base_url, second_url, third_url],
            "afterBack": after_back,
            "afterForward": after_forward,
            "elementIdPresent": bool(element.id),
        }


def _cross_document_stale_element(
    target: WebDriverTarget,
    fixture: str,
) -> dict[str, Any]:
    with _driver(target) as driver:
        driver.get(f"{fixture}/webdriver/basic?document=first")
        element = driver.find_element(By.ID, "main")
        driver.get(f"{fixture}/webdriver/frame?document=second")
        stale_error = _expect_exception(
            StaleElementReferenceException,
            lambda: element.tag_name,
        )
        assert_equal(
            driver.find_element(By.ID, "inside-frame").text,
            "frame ready",
            "session remains usable after stale element error",
        )
        return {"staleError": stale_error, "currentUrl": driver.current_url}


def _top_level_storage_namespaces(
    target: WebDriverTarget,
    fixture: str,
) -> dict[str, Any]:
    with _driver(target) as driver:
        first_url = f"{fixture}/webdriver/basic?storage=first"
        second_url = f"{fixture}/webdriver/basic?storage=second"
        driver.get(first_url)
        first_handle = driver.current_window_handle
        driver.execute_script(
            "localStorage.clear(); sessionStorage.clear(); "
            "localStorage.setItem('semantic-local', 'shared'); "
            "sessionStorage.setItem('semantic-session', 'first');"
        )

        driver.switch_to.new_window("tab")
        second_handle = driver.current_window_handle
        driver.get(second_url)
        second_initial = driver.execute_script(
            "return {local: localStorage.getItem('semantic-local'), "
            "session: sessionStorage.getItem('semantic-session')};"
        )
        assert_equal(
            second_initial,
            {"local": "shared", "session": None},
            "second top-level storage",
        )
        driver.execute_script("sessionStorage.setItem('semantic-session', 'second')")
        driver.refresh()
        second_after_reload = driver.execute_script(
            "return sessionStorage.getItem('semantic-session')"
        )

        driver.switch_to.window(first_handle)
        driver.refresh()
        first_after_reload = driver.execute_script(
            "return sessionStorage.getItem('semantic-session')"
        )
        assert_equal(first_after_reload, "first", "first sessionStorage after reload")
        assert_equal(second_after_reload, "second", "second sessionStorage after reload")
        return {
            "handlesDistinct": first_handle != second_handle,
            "secondInitial": second_initial,
            "afterReload": {
                "first": first_after_reload,
                "second": second_after_reload,
            },
        }


def _popup_storage_snapshot(
    target: WebDriverTarget,
    fixture: str,
) -> dict[str, Any]:
    with _driver(target) as driver:
        driver.get(f"{fixture}/webdriver/basic?popup=opener")
        opener = driver.current_window_handle
        driver.execute_script(
            "sessionStorage.clear(); sessionStorage.setItem('semantic-popup', 'opener')"
        )
        popup_url = f"{fixture}/webdriver/basic?popup=child"
        driver.execute_script("window.open(arguments[0], 'semantic-popup')", popup_url)
        WebDriverWait(driver, 5, poll_frequency=0.05).until(
            lambda current: len(current.window_handles) == 2
        )
        popup = next(handle for handle in driver.window_handles if handle != opener)
        driver.switch_to.window(popup)
        _wait_for_url(driver, popup_url)
        popup_initial = driver.execute_script(
            "return sessionStorage.getItem('semantic-popup')"
        )
        assert_equal(popup_initial, "opener", "popup initial sessionStorage snapshot")
        driver.execute_script("sessionStorage.setItem('semantic-popup', 'popup')")

        driver.switch_to.window(opener)
        opener_after_popup = driver.execute_script(
            "return sessionStorage.getItem('semantic-popup')"
        )
        assert_equal(
            opener_after_popup,
            "opener",
            "opener sessionStorage after popup mutation",
        )
        return {
            "handlesDistinct": opener != popup,
            "popupInitial": popup_initial,
            "openerAfterPopupMutation": opener_after_popup,
        }


def _nested_frame_routing_and_recovery(
    target: WebDriverTarget,
    fixture: str,
) -> dict[str, Any]:
    with _driver(target) as driver:
        driver.get(f"{fixture}/webdriver/nested-frames")
        outer_element = driver.find_element(By.ID, "outerById")
        driver.switch_to.frame("outerById")
        assert_equal(
            driver.find_element(By.ID, "outer-main").text,
            "outer ready",
            "outer frame by id",
        )
        driver.switch_to.frame("innerByName")
        assert_equal(
            driver.find_element(By.ID, "inner-main").text,
            "inner ready",
            "inner frame by name",
        )
        driver.switch_to.parent_frame()
        assert_equal(
            driver.find_element(By.ID, "outer-main").text,
            "outer ready",
            "parent frame recovery",
        )
        driver.switch_to.default_content()
        driver.switch_to.frame(outer_element)
        assert_equal(
            driver.find_element(By.ID, "outer-main").text,
            "outer ready",
            "outer frame by WebElement",
        )
        driver.switch_to.default_content()
        driver.execute_script("document.querySelector('#outerById').remove()")
        missing_error = _expect_exception(
            NoSuchFrameException,
            lambda: driver.switch_to.frame("outerById"),
        )
        driver.switch_to.frame("siblingByName")
        sibling_text = driver.find_element(By.ID, "sibling-main").text
        assert_equal(sibling_text, "sibling ready", "sibling frame after outer removal")
        return {
            "missingFrameError": missing_error,
            "siblingText": sibling_text,
        }


def _shadow_frame_window_named_access(
    target: WebDriverTarget,
    fixture: str,
) -> dict[str, Any]:
    with _driver(target) as driver:
        driver.get(f"{fixture}/webdriver/semantic-shadow-frame")
        observed = driver.execute_script(
            """
            const frame = document.querySelector('#shadow-frame-host').shadowRoot
              .querySelector('#shadowFrameId');
            return {
              hasContentWindow: !!frame.contentWindow,
              nameInWindow: 'shadowFrameName' in window,
              idInWindow: 'shadowFrameId' in window,
              nameType: typeof window.shadowFrameName,
              idType: typeof window.shadowFrameId,
            };
            """
        )
        assert_equal(observed["hasContentWindow"], True, "shadow iframe contentWindow")
        assert_equal(observed["nameInWindow"], False, "shadow iframe name in Window")
        assert_equal(observed["idInWindow"], False, "shadow iframe id in Window")
        assert_equal(observed["nameType"], "undefined", "shadow iframe name type")
        assert_equal(observed["idType"], "undefined", "shadow iframe id type")
        return observed


def _dialog_return_values(
    target: WebDriverTarget,
    fixture: str,
) -> dict[str, Any]:
    with _driver(target) as driver:
        driver.get(f"{fixture}/webdriver/basic?dialogs=1")
        driver.execute_script(
            "window.__dialogResults = {}; "
            "setTimeout(() => { window.__dialogResults.confirmDismiss = confirm('confirm dismiss'); }, 0);"
        )
        alert = WebDriverWait(driver, 5).until(EC.alert_is_present())
        assert_equal(alert.text, "confirm dismiss", "dismissed confirm text")
        alert.dismiss()
        confirm_dismiss = driver.execute_script(
            "return window.__dialogResults.confirmDismiss"
        )

        driver.execute_script(
            "setTimeout(() => { window.__dialogResults.confirmAccept = confirm('confirm accept'); }, 0);"
        )
        alert = WebDriverWait(driver, 5).until(EC.alert_is_present())
        alert.accept()
        confirm_accept = driver.execute_script(
            "return window.__dialogResults.confirmAccept"
        )

        driver.execute_script(
            "setTimeout(() => { window.__dialogResults.promptAccept = prompt('prompt accept', 'default'); }, 0);"
        )
        alert = WebDriverWait(driver, 5).until(EC.alert_is_present())
        alert.send_keys("entered")
        alert.accept()
        prompt_accept = driver.execute_script(
            "return window.__dialogResults.promptAccept"
        )

        driver.execute_script(
            "setTimeout(() => { window.__dialogResults.promptDismiss = prompt('prompt dismiss', 'default'); }, 0);"
        )
        alert = WebDriverWait(driver, 5).until(EC.alert_is_present())
        alert.dismiss()
        prompt_dismiss = driver.execute_script(
            "return window.__dialogResults.promptDismiss"
        )

        observed = {
            "confirmDismiss": confirm_dismiss,
            "confirmAccept": confirm_accept,
            "promptAccept": prompt_accept,
            "promptDismiss": prompt_dismiss,
        }
        assert_equal(
            observed,
            {
                "confirmDismiss": False,
                "confirmAccept": True,
                "promptAccept": "entered",
                "promptDismiss": None,
            },
            "dialog return values",
        )
        return observed


def _standard_error_types(
    target: WebDriverTarget,
    fixture: str,
) -> dict[str, Any]:
    with _driver(target) as driver:
        driver.get(f"{fixture}/webdriver/basic?errors=first")
        stale = driver.find_element(By.ID, "main")
        errors = {
            "invalidSelector": _expect_exception(
                InvalidSelectorException,
                lambda: driver.find_element(By.CSS_SELECTOR, "["),
            ),
            "noSuchFrame": _expect_exception(
                NoSuchFrameException,
                lambda: driver.switch_to.frame("semantic-missing-frame"),
            ),
            "noSuchWindow": _expect_exception(
                NoSuchWindowException,
                lambda: driver.switch_to.window("semantic-missing-window"),
            ),
        }
        driver.get(f"{fixture}/webdriver/frame?errors=second")
        errors["staleElement"] = _expect_exception(
            StaleElementReferenceException,
            lambda: stale.tag_name,
        )
        assert_true(
            driver.find_element(By.ID, "inside-frame").is_displayed(),
            "session should remain usable after standard errors",
        )
        return {"errors": errors, "currentUrl": driver.current_url}
