/**
 * Collects the {@link File} list behind one file drop, descending into any
 * dropped folders. A plain `dataTransfer.files` read flattens nothing past the
 * top level — a dropped directory yields no usable members — so the browser's
 * entry API walks the dropped subtree instead.
 * @module
 */

/**
 * Every file under one drop's transfer data, folders included, in entry order.
 *
 * Entry handles are only valid during the drop event, so
 * {@link DataTransferItem.webkitGetAsEntry} runs synchronously before any
 * await. Transfers without entries (text drags aside, engines vary) fall back
 * to the flat `files` list, as does a traversal that collects nothing.
 * @param dataTransfer - the drop's transfer data.
 * @returns the collected files; possibly empty.
 */
export async function filesFromDataTransfer(dataTransfer: DataTransfer): Promise<readonly File[]> {
  const items = Array.from(dataTransfer.items ?? [])
  const entries = items.map(item => typeof item.webkitGetAsEntry === 'function' ? item.webkitGetAsEntry() : null)
  if (entries.length === 0 || entries.every(entry => entry === null)) {
    return Array.from(dataTransfer.files ?? [])
  }
  const files: File[] = []
  // One visited set across the whole drop: engine entry objects differ per
  // item even for the same path, but a repeated object identity is the only
  // cycle signal reachable here, and it is what a self-nesting tree produces.
  const visited = new Set<FileSystemEntry>()
  for (const entry of entries) {
    if (entry !== null) await collectEntryFiles(entry, files, visited)
  }
  return files.length > 0 ? files : Array.from(dataTransfer.files ?? [])
}

/** Recursively append one entry's files; directories are walked once. */
async function collectEntryFiles(
  entry: FileSystemEntry,
  files: File[],
  visited: Set<FileSystemEntry>,
): Promise<void> {
  if (entry.isFile) {
    const file = await entryFile(entry as FileSystemFileEntry)
    if (file !== null) files.push(file)
    return
  }
  if (!entry.isDirectory || visited.has(entry)) return
  visited.add(entry)
  const reader = (entry as FileSystemDirectoryEntry).createReader()
  // readEntries hands over at most a page per call and must be called again
  // until an empty page; an error ends this directory, not the whole drop.
  for (;;) {
    const batch = await readerPage(reader)
    if (batch.length === 0) break
    for (const child of batch) await collectEntryFiles(child, files, visited)
  }
}

/** One entry's file, or null when the browser cannot produce it. */
function entryFile(entry: FileSystemFileEntry): Promise<File | null> {
  return new Promise((resolve) => { entry.file(resolve, () => resolve(null)) })
}

/** One readEntries page; an errored directory resolves as the empty end page. */
function readerPage(reader: FileSystemDirectoryReader): Promise<readonly FileSystemEntry[]> {
  return new Promise((resolve) => { reader.readEntries(resolve, () => resolve([])) })
}
