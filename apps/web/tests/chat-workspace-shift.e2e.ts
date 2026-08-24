// Web e2e scenario: a workspace-less chat shifts into a workspace, keeping
// its conversation.
//
// Boot with zero Workspaces auto-connects the chat session; a seeded one-turn
// ungrouped conversation is opened from the sidebar, and the composer's chat
// chip — the active-phase shift affordance — raises the add-workspace flow.
// Adopting a directory forks the session into that Workspace: the transcript
// survives in the opened child (retention, not a move), the child groups
// under the new Workspace in the sidebar, and the source stays ungrouped.
//
// Zero model calls: seeding, listing, creating, and forking are persistence
// and host RPCs with no model involvement. A stray stream would fail loud
// with NO_ADAPTER on the open llm seam.
import { mkdir } from 'node:fs/promises'
import { join } from 'node:path'
import type { Browser, Page } from 'playwright'
import { chromium } from 'playwright'
import { afterAll, beforeAll, describe, expect, it, onTestFailed } from 'vitest'
import {
  launchWebScaffold, seedSession, watchConsole, type WebScaffold,
} from './scaffold.ts'
import { newEnglishPage, saveFailureShot } from './support.ts'

const SESSION_ID = 'chat-shift-web-e2e'
// A cold session lists under its cwd-basename fallback until an open folds
// the log-backed title, so the row is addressed by this deterministic name.
const COLD_ROW_NAME = 'chat-shift-source'
const MESSAGE_TEXT = 'Remember the number forty-two.'
const TARGET_NAME = 'shift-target'

/**
 * A settled one-turn session with no model content: the shift scenario pins
 * retention of a real conversation, not any particular assistant wording.
 * @returns a tokenized session log ending on a closed turn.
 */
function seedLog(): string {
  const time = 1784974100000
  const at = (index: number, event: Record<string, unknown>): string =>
    JSON.stringify({ ...event, seq: index, time: time + index })
  return [
    JSON.stringify({ type: 'session', version: 0, id: '{{sessionId}}', createdAt: time, cwd: `{{cwd}}/${COLD_ROW_NAME}` }),
    at(0, { type: 'turn/start', data: { turn: 1, trigger: { kind: 'message', source: { kind: 'user', rpcId: 'seed' } } } }),
    at(1, {
      type: 'user/message',
      data: { content: [{ type: 'text', text: MESSAGE_TEXT }], source: { kind: 'user', rpcId: 'seed' } },
      surfaceOp: 'append',
    }),
    at(2, { type: 'session/title', data: { title: 'Seeded turn', messageSeqs: [1], source: { kind: 'fallback' } } }),
    at(3, { type: 'turn/end', data: { turn: 1, reason: { kind: 'completed' } } }),
  ].join('\n')
}

describe('web e2e: chat shifts into a workspace retaining history', () => {
  let scaffold: WebScaffold
  let browser: Browser
  let page: Page
  let tripwire: ReturnType<typeof watchConsole>

  beforeAll(async () => {
    scaffold = await launchWebScaffold({})
    await seedSession(scaffold, seedLog(), SESSION_ID)

    browser = await chromium.launch()
    page = await newEnglishPage(browser)
    tripwire = watchConsole(page)
    await page.goto(scaffold.baseUrl, { waitUntil: 'load' })
    await page.waitForSelector('[class*="frame"]', { timeout: 30_000 })
  }, 120_000)

  afterAll(async () => {
    await browser?.close()
    await scaffold?.close()
  })

  /** Expand a group row by its visible label and return its session rows. */
  const groupRows = async (label: string): Promise<number> => {
    const row = page.getByRole('treeitem').filter({ hasText: label })
    if (await row.first().getAttribute('aria-expanded') !== 'true') {
      await row.first().click()
    }
    return row.locator('..').locator('[role="treeitem"]').count()
  }

  it('moves an ongoing chat into an adopted workspace without losing the transcript', async () => {
    onTestFailed(() => saveFailureShot(page, 'web-e2e-chat-workspace-shift'))
    // Boot settles in the auto-connected chat session; no Workspace exists.
    const tree = page.getByRole('tree', { name: 'Sessions' })
    await tree.waitFor({ timeout: 30_000 })
    await page.locator('textarea:enabled[placeholder="Describe what you want to build"]')
      .waitFor({ timeout: 15_000 })

    // Open the seeded conversation: it has history, so it renders as an
    // active session whose only workspace entry is the composer chip.
    await tree.getByText(COLD_ROW_NAME, { exact: true }).click()
    await page.getByText(MESSAGE_TEXT, { exact: true }).waitFor({ timeout: 15_000 })
    const rowsBefore = await tree.locator('[role="treeitem"]').count()

    // The chat chip raises the picker; with nothing listed, the add flow IS
    // the menu (the anchor gesture opens the directory dialog directly).
    const chip = page.getByRole('button', { name: 'Choose workspace' })
    await chip.waitFor({ timeout: 15_000 })
    await expect(chip.locator('..')).toContainText('Chat')
    await chip.click()
    const dialog = page.getByRole('dialog', { name: 'Select Workspace Directory' })
    await dialog.waitFor({ timeout: 10_000 })
    await dialog.getByRole('button', { name: 'Edit path' }).click()
    const target = join(scaffold.workspaceCwd, TARGET_NAME)
    await mkdir(target, { recursive: true })
    const pathInput = dialog.getByRole('textbox', { name: 'Edit path' })
    await pathInput.fill(target)
    await pathInput.press('Enter')
    await dialog.getByRole('button', { name: 'Open', exact: true }).click()

    // The retargeted fork lands: the child is open and live, the seeded
    // message is still on screen (context retained), the sidebar grows by
    // the Workspace group plus its session, and the source stays ungrouped.
    await page.locator('textarea:enabled[placeholder="Describe what you want to build"]')
      .waitFor({ timeout: 15_000 })
    await page.getByText(MESSAGE_TEXT, { exact: true }).waitFor({ timeout: 15_000 })
    expect(await groupRows(TARGET_NAME)).toBe(1)
    expect(await tree.locator('[role="treeitem"]').count()).toBe(rowsBefore + 2)
    expect(tripwire.pageErrors).toEqual([])
  }, 120_000)
})
