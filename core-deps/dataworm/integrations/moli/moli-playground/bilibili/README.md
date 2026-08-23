# Bilibili Moli CDP Demo

This demo starts `moli serve`, connects to it with raw CDP, opens
`https://www.bilibili.com/`, fills the search box with `猛禽峡谷`, clicks the
search button, and prints the first five video titles from the result page.

If Bilibili's homepage click handler does not complete navigation inside the
short click timeout, the script falls back to the equivalent search URL after
the click attempt. This keeps the demo useful while still exercising the
homepage input and button path first.

## Run

From this directory:

```bash
uv run --with websockets python bilibili_cdp_demo.py
```

From the repository root:

```bash
uv run --with websockets python moli-playground/bilibili/bilibili_cdp_demo.py
```

Useful options:

- `--keyword "猛禽峡谷"`
- `--limit 5`
- `--moli-bin ../../target/release/moli`
- `--profile-dir .profile`
- `--http-proxy http://127.0.0.1:7890`
- `--cdp-endpoint http://127.0.0.1:9222` to reuse an existing `moli serve`

Set `MOLI_PLAYGROUND_TRACE_BG=1` to print background `moli serve`
logs while the demo runs.
