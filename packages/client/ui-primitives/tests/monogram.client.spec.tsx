// @vitest-environment jsdom
import { cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { Monogram, monogramLetters } from '@deepseek-ai/dsh-client-ui-primitives'

afterEach(cleanup)

describe('monogramLetters', () => {
  it.each([
    ['Anthropic', 'A'],
    ['Amazon Bedrock', 'AB'],
    ['Moonshot AI', 'MA'],
    ['Moonshot AI CN', 'MC'],
    ['Cloudflare Workers AI', 'CA'],
    ['Google Vertex AI', 'GA'],
    ['Qwen Token Plan Individual', 'QI'],
    ['OpenAI Codex (legacy)', 'OC'],
    ['Z.AI', 'Z'],
    ['xAI', 'X'],
    ['NVIDIA', 'N'],
  ])('marks the installed provider name %s as %s', (name, letters) => {
    expect(monogramLetters(name)).toBe(letters)
  })

  it('takes code points, not UTF-16 units, so a non-Latin name keeps one character', () => {
    expect(monogramLetters('月之暗面')).toBe('月')
    // A leading symbol is not a character the mark can draw, so the next word
    // supplies it instead of the mark showing an emoji.
    expect(monogramLetters('🚀 Launch')).toBe('L')
  })

  it('ignores surrounding and repeated whitespace', () => {
    expect(monogramLetters('  Open   Router  ')).toBe('OR')
  })

  it.each([
    ['', ''],
    ['   ', ''],
    ['(draft)', ''],
    ['…?!', ''],
    ['7', '7'],
    ['1111 Labs', '1L'],
  ])('draws nothing or the leading character for %j', (name, letters) => {
    expect(monogramLetters(name)).toBe(letters)
  })

  it('drops a leading bracket even when a qualifier comes first', () => {
    // A name somebody typed with a leading parenthetical has no second word to
    // pair with, so the mark falls back to the first unbracketed word.
    expect(monogramLetters('(beta) Fireworks')).toBe('F')
    expect(monogramLetters('(beta)')).toBe('')
  })
})

describe('Monogram', () => {
  it('draws the derived letters and hides the mark from the accessibility tree', () => {
    const { container } = render(<Monogram name="Amazon Bedrock" />)
    const mark = container.firstElementChild

    expect(mark?.textContent).toBe('AB')
    // The label beside the mark carries the identity, so the mark must not be
    // announced a second time.
    expect(mark?.getAttribute('aria-hidden')).toBe('true')
    expect(container.querySelector('[role]')).toBeNull()
  })

  it('scales the letterform from the requested edge', () => {
    const { container } = render(<Monogram name="Groq" size={32} />)
    const mark = container.firstElementChild as HTMLElement

    expect(mark.style.getPropertyValue('--monogram-edge')).toBe('32px')
    expect(mark.style.getPropertyValue('--monogram-letter')).toBe('13px')
  })

  it('renders nothing when the name yields no character', () => {
    const { container } = render(<Monogram name="…" />)

    expect(container.firstChild).toBeNull()
  })

  it('forwards the layout class to the mark', () => {
    const { container } = render(<Monogram name="Groq" className="row-mark" />)

    expect(container.firstElementChild?.getAttribute('class')).toContain('row-mark')
  })
})
