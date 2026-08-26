import { createHash } from 'node:crypto'
import { describe, expect, it } from 'vitest'
import {
  SidecarUpdateError,
  downloadBytes,
  fetchLatestRelease,
  parseRelease,
  parseSha256Sums,
  verifyChecksum,
} from '../src/github.ts'
import { startFakeGithub } from './fake-github.ts'

const FULL_RELEASE = {
  tag_name: 'v1.2.0',
  name: 'Release v1.2.0',
  html_url: 'https://example.invalid/releases/v1.2.0',
  published_at: '2026-08-01T00:00:00Z',
  assets: [
    { name: 'ai-sidecar-x64', browser_download_url: 'https://example.invalid/a' },
    { junk: true },
    { name: 'no-url' },
    null,
  ],
}

describe('parseRelease', () => {
  it('maps the fields the pipeline uses and drops malformed asset rows', () => {
    const release = parseRelease(FULL_RELEASE)
    expect(release.tag).toBe('v1.2.0')
    expect(release.name).toBe('Release v1.2.0')
    expect(release.url).toBe('https://example.invalid/releases/v1.2.0')
    expect(release.publishedAt).toBe('2026-08-01T00:00:00Z')
    expect(release.assets).toEqual([{ name: 'ai-sidecar-x64', url: 'https://example.invalid/a' }])
  })

  it('keeps optional fields absent when empty', () => {
    const release = parseRelease({ tag_name: 'v1', assets: [] })
    expect(release).toEqual({ tag: 'v1', assets: [] })
    const padded = parseRelease({ tag_name: 'v1', name: '', assets: [] })
    expect(padded.name).toBeUndefined()
  })

  it.each([
    ['array root', [1]],
    ['null root', null],
    ['missing tag', { assets: [] }],
    ['blank tag', { tag_name: '', assets: [] }],
    ['missing assets', { tag_name: 'v1' }],
  ])('rejects %s with RELEASE_MALFORMED', (_label, body) => {
    expect(() => parseRelease(body)).toThrow(SidecarUpdateError)
    try {
      parseRelease(body)
    } catch (error) {
      expect((error as SidecarUpdateError).code).toBe('RELEASE_MALFORMED')
    }
  })
})

describe('parseSha256Sums', () => {
  it('parses digest lines and skips junk', () => {
    const sums = parseSha256Sums([
      '# comment line',
      '',
      'ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789  plain-name',
      '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef *binary-name',
      'tooshort  skipped',
      '   ',
    ].join('\n'))
    expect(sums.get('plain-name')).toBe('abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789')
    expect(sums.get('binary-name')).toBe('0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef')
    expect(sums.size).toBe(2)
  })
})

describe('verifyChecksum', () => {
  const bytes = new Uint8Array([1, 2, 3])
  const digest = createHash('sha256').update(bytes).digest('hex')

  it('accepts matching bytes case-insensitively', () => {
    expect(() => verifyChecksum(bytes, digest.toUpperCase())).not.toThrow()
  })

  it('rejects a mismatch with CHECKSUM_MISMATCH', () => {
    try {
      verifyChecksum(bytes, `${digest.slice(0, -1)}0`)
      expect.unreachable()
    } catch (error) {
      expect((error as SidecarUpdateError).code).toBe('CHECKSUM_MISMATCH')
    }
  })
})

describe('wire client against a local server', () => {
  it('fetches and validates a release document', async () => {
    const server = await startFakeGithub({ extraAssetRows: [{ junk: true }] })
    try {
      const release = await fetchLatestRelease(server.url, 'owner/repo')
      expect(release.tag).toBe('v1.2.0')
      expect(release.assets.map(asset => asset.name)).toContain('SHA256SUMS')
    } finally {
      await server.close()
    }
  })

  it('reports HTTP failures as RELEASE_LOOKUP', async () => {
    const server = await startFakeGithub({ releaseStatus: 404 })
    try {
      await expect(fetchLatestRelease(server.url, 'owner/repo'))
        .rejects.toThrow(/answered 404/)
      await expect(fetchLatestRelease(server.url, 'owner/repo'))
        .rejects.toMatchObject({ code: 'RELEASE_LOOKUP' })
    } finally {
      await server.close()
    }
  })

  it('reports transport failures as RELEASE_LOOKUP with their cause', async () => {
    await expect(fetchLatestRelease('http://127.0.0.1:9', 'owner/repo'))
      .rejects.toMatchObject({ code: 'RELEASE_LOOKUP' })
  })

  it('reports non-JSON bodies as RELEASE_MALFORMED', async () => {
    const server = await startFakeGithub({ releaseBody: '<html>' })
    try {
      await expect(fetchLatestRelease(server.url, 'owner/repo'))
        .rejects.toMatchObject({ code: 'RELEASE_MALFORMED' })
    } finally {
      await server.close()
    }
  })

  it('downloads asset bytes and reports failures as DOWNLOAD_FAILED', async () => {
    const server = await startFakeGithub()
    try {
      const response = await fetch(`${server.url}/repos/owner/repo/releases/latest`)
      const release = parseRelease(await response.json())
      const asset = release.assets.find(entry => entry.name !== 'SHA256SUMS')
      if (asset === undefined) throw new Error('fixture served no binary asset')
      const bytes = await downloadBytes(asset.url)
      expect(bytes.byteLength).toBeGreaterThan(0)
      await expect(downloadBytes(`${server.url}/assets/absent`))
        .rejects.toMatchObject({ code: 'DOWNLOAD_FAILED' })
      await expect(downloadBytes('http://127.0.0.1:9/assets/x'))
        .rejects.toMatchObject({ code: 'DOWNLOAD_FAILED' })
    } finally {
      await server.close()
    }
  })
})
