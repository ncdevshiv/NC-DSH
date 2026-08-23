from __future__ import annotations

from selenium.webdriver.chrome.options import Options as ChromeOptions
from selenium.webdriver.common.options import ArgOptions

from .config import WebDriverTarget


def create_selenium_options(
    target: WebDriverTarget,
    *,
    enable_downloads: bool = False,
    requested_browser_name: str | None = None,
) -> ArgOptions:
    if target.browser_name == "chrome":
        options = ChromeOptions()
        if target.browser_binary is not None:
            options.binary_location = str(target.browser_binary)
        options.add_argument("--headless=new")
        options.add_argument("--no-sandbox")
        options.add_argument("--disable-gpu")
    else:
        options = ArgOptions()

    options.set_capability(
        "browserName",
        requested_browser_name or target.browser_name,
    )
    options.set_capability("pageLoadStrategy", "normal")
    options.enable_downloads = enable_downloads
    return options
