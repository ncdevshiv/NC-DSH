// @vitest-environment jsdom

import { describe, expect, it } from 'vitest'
import { filesFromDataTransfer } from '../src/client/drop-files.ts'

/** Minimal image file stand-in. */
function png(name: string): File {
  return new File([Uint8Array.of(1)], name, { type: 'image/png' })
}

/** Entry stub resolving file() immediately. */
function fileEntry(file: File): FileSystemFileEntry {
  return {
    isFile: true, isDirectory: false, name: file.name, fullPath: `/${file.name}`,
    file: (success: (data: File) => void) => { success(file) },
  } as unknown as FileSystemFileEntry
}

/**
 * Directory entry stub serving `pages` as successive readEntries pages
 * (`failRead` turns every page read into an error instead).
 */
function dirEntry(
  name: string,
  pages: readonly FileSystemEntry[][],
  failRead = false,
): FileSystemDirectoryEntry {
  let page = 0
  return {
    isFile: false, isDirectory: true, name, fullPath: `/${name}`,
    createReader: () => ({
      readEntries: (success: (entries: FileSystemEntry[]) => void, error?: (err: DOMException) => void) => {
        if (failRead) {
          error?.(new DOMException('reader failed'))
          return
        }
        success([...(page < pages.length ? pages[page]! : [])])
        page += 1
      },
    }),
  } as unknown as FileSystemDirectoryEntry
}

/** One-item transfer carrying an entry; `entry` null models engines without the entry API. */
function itemTransfer(entry: FileSystemEntry | null, files: readonly File[] = []): DataTransfer {
  return {
    items: [{ webkitGetAsEntry: () => entry }],
    files,
  } as unknown as DataTransfer
}

describe('filesFromDataTransfer', () => {
  it('falls back to the flat files list when no item exposes an entry', async () => {
    const flat = [png('flat.png')]
    const none = await filesFromDataTransfer({ files: flat } as unknown as DataTransfer)
    expect(none).toEqual(flat)
    const noMethod = await filesFromDataTransfer({
      items: [{}],
      files: flat,
    } as unknown as DataTransfer)
    expect(noMethod).toEqual(flat)
  })

  it('collects dropped files through their entries instead of the empty files list', async () => {
    const a = png('a.png')
    const b = png('b.png')
    const files = await filesFromDataTransfer(itemTransfer(fileEntry(a), []))
    expect(files).toEqual([a])
    const pair = await filesFromDataTransfer({
      items: [{ webkitGetAsEntry: () => fileEntry(a) }, { webkitGetAsEntry: () => fileEntry(b) }],
      files: [],
    } as unknown as DataTransfer)
    expect(pair).toEqual([a, b])
  })

  it('walks nested directories across chunked readEntries pages', async () => {
    const leaf = png('leaf.png')
    const deep = png('deep.png')
    const root = dirEntry('root', [
      [fileEntry(leaf), dirEntry('nested', [[fileEntry(deep)]])],
      [fileEntry(png('second.png'))],
    ])
    const files = await filesFromDataTransfer(itemTransfer(root))
    expect(files.map(file => file.name)).toEqual(['leaf.png', 'deep.png', 'second.png'])
  })

  it('skips unreadable files and errored directories while keeping sibling branches', async () => {
    const good = png('good.png')
    const brokenFile = {
      isFile: true, isDirectory: false, name: 'broken.png',
      file: (_success: (file: File) => void, error?: (err: DOMException) => void) => {
        error?.(new DOMException('unreadable'))
      },
    } as unknown as FileSystemFileEntry
    const root = dirEntry('root', [
      [brokenFile, dirEntry('broken-dir', [], true)],
      [fileEntry(good)],
    ])
    const files = await filesFromDataTransfer(itemTransfer(root))
    expect(files).toEqual([good])
  })

  it('visits a repeated directory object once, so self-nesting trees terminate', async () => {
    const inner = png('inner.png')
    const nested = dirEntry('nested', [[fileEntry(inner)]])
    const outer = dirEntry('outer', [
      [fileEntry(png('top.png'))],
      // The same object appearing again (a cycle) is walked once.
      [nested, nested],
    ])
    const files = await filesFromDataTransfer(itemTransfer(outer))
    expect(files.map(file => file.name)).toEqual(['top.png', 'inner.png'])
  })

  it('falls back to the flat files list when a traversal collects nothing', async () => {
    const flat = [png('fallback.png')]
    const emptyDir = dirEntry('empty', [[]])
    const files = await filesFromDataTransfer(itemTransfer(emptyDir, flat))
    expect(files).toEqual(flat)
  })

  it('skips items without entries inside a mixed transfer and ignores non-file non-directory entries', async () => {
    const a = png('mixed.png')
    const mixed = await filesFromDataTransfer({
      items: [{ webkitGetAsEntry: () => fileEntry(a) }, {}],
      files: [],
    } as unknown as DataTransfer)
    expect(mixed).toEqual([a])
    // An entry that is neither file nor directory contributes nothing.
    const stray = await filesFromDataTransfer({
      items: [{ webkitGetAsEntry: () => ({ isFile: false, isDirectory: false, name: 'stray' }) }],
      files: [],
    } as unknown as DataTransfer)
    expect(stray).toEqual([])
  })
})
