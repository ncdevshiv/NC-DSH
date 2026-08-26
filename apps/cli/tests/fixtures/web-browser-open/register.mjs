import { existsSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const exitMarker = join(process.cwd(), `.dsh-browser-open-${process.pid}`)

const markerPoll = setInterval(() => {
  if (!existsSync(exitMarker)) return
  rmSync(exitMarker, { force: true })
  process.exit(0)
}, 25)
markerPoll.unref()

// The fixture swaps the web-app opener seam before the booted tree activates:
// same assembled server, same readiness ordering, no real browser window.
const webAppEntry = pathToFileURL(join(
  fileURLToPath(new URL('../../../../../', import.meta.url)),
  'packages/bundle/web-app/lib/index.js',
))
const { internals } = await import(webAppEntry)
if (typeof internals?.openBrowser !== 'function') {
  throw new Error('browser-open fixture: packages/bundle/web-app/lib is not built; run bun run build first')
}

internals.openBrowser = async (url) => {
  if (process.env.BROWSER_OPEN_TEST_FAILURE !== undefined) {
    throw new Error(process.env.BROWSER_OPEN_TEST_FAILURE)
  }
  const response = await fetch(url)
  const html = await response.text()
  console.log(`dsh browser-open: ${JSON.stringify({
    url,
    status: response.status,
    bootManifest: html.includes('__DSH_BOOT__'),
  })}`)
}

// The SSH case has no opener helper to stop the long-lived Web process.
if (process.env.DSH_BROWSER_OPEN_TEST_EXIT_ON_READY === '1') {
  const originalLog = console.log
  console.log = (...args) => {
    originalLog(...args)
    if (typeof args[0] === 'string' && args[0].startsWith('dsh web: ')) {
      setTimeout(() => process.exit(0), 250)
    }
  }
}

if (process.env.DSH_BROWSER_OPEN_TEST_EXIT_ON_FAILURE === '1') {
  const originalError = console.error
  console.error = (...args) => {
    originalError(...args)
    if (typeof args[0] === 'string' && args[0].startsWith('web-app: could not open the default browser because ')) {
      setTimeout(() => process.exit(0), 0)
    }
  }
}
