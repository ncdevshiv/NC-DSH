# wptrunner-moli

This package registers a `moli` product for upstream WPT `wptrunner`.

It is intentionally scoped to `wdspec` for now. The goal is to run WebDriver
Classic/BiDi WPTs against `moli serve` without borrowing another browser
product such as Servo.

Use `scripts/moli-wpt-run.sh` from the repository root for normal local
runs. It calls upstream `wptrunner` directly because upstream `./wpt run` has a
hosts-file preflight tied to built-in products; an external `moli` product
is blocked before product startup.

The wrapper defaults `PYTEST_ADDOPTS` to include `--asyncio-mode=auto` unless an
asyncio mode was already supplied. This keeps local upstream WPT checkouts that
miss explicit `pytest.mark.asyncio` markers from turning async wdspec tests into
PytestExecutor harness skips. Set `PYTEST_ADDOPTS=--asyncio-mode=strict` when a
strict marker audit is desired.

The Moli product passes `--layout --resource` to `moli serve` by
default. WPT WebDriver/BiDi tests therefore exercise the real layout policy,
and network tests observe optional image,
font, media, and text-track requests the same way they observe script and
stylesheet requests, while Moli's normal browser default keeps those
resource families disabled for lower resource use. Pass
`--moli-no-image-fetch` to leave only image fetching disabled while
retaining the other WPT parity opt-ins.

Install it into the WPT Python environment with `uv`:

```bash
uv pip install --python ../wpt/.venv-moli/bin/python -e tools/wptrunner-moli
```

Example:

```bash
MOLI_BIN=target/debug/moli \
  scripts/moli-wpt-run.sh \
  webdriver/tests/bidi/network/add_data_collector/user_contexts.py
```
