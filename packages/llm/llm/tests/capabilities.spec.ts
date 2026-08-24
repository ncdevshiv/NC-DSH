import { describe, expect, it } from 'vitest'
import { inferModelModalities } from '@deepseek-ai/dsh-llm'

describe('model modality inference', () => {
  it('returns image support for known multimodal families by id prefix', () => {
    for (const id of [
      'gemini-1.5-pro',
      'gemini-2.5-flash',
      'gpt-4o',
      'gpt-4o-2025-05-12',
      'gpt-4.1-2025-04-29',
      'gpt-5.5',
      'claude-sonnet-4-20250514',
      'deepseek-v3',
      'deepseek-v3-0324',
      'qwen-vl-max',
      'qwen2.5-vl-72b',
      'qwen3-max',
      'glm-4-plus',
      'glm-4-flash',
      'kimi-k2',
      'doubao-1.6-pro',
      'minimax-m2.6',
      'hunyuan-vision',
      'yi-vl-plus',
      'step-2-16k',
    ]) {
      const result = inferModelModalities(id)
      expect(result, `expected image for ${id}`).toEqual(['text', 'image'])
    }
  })

  it('falls back to the display name when the id is bare', () => {
    expect(inferModelModalities('gemini', 'Gemini-2.5-Pro')).toEqual(['text', 'image'])
    expect(inferModelModalities('model-1', 'gpt-4o')).toEqual(['text', 'image'])
  })

  it('is case-insensitive', () => {
    expect(inferModelModalities('GPT-4o')).toEqual(['text', 'image'])
    expect(inferModelModalities('GEMINI-2.5-Flash')).toEqual(['text', 'image'])
    expect(inferModelModalities('DEEPSEEK-V3')).toEqual(['text', 'image'])
  })

  it.each([
    'llama-3-1-8b',
    'deepseek-v2-lite',
    'qwen2-72b',
    'custom-model',
  ])('leaves unrecognized ids undefined so the caller keeps its text-only default', (id) => {
    expect(inferModelModalities(id)).toBeUndefined()
  })

  it('prefers a name match over a bare id when both collide', () => {
    expect(inferModelModalities('gemini', 'gemini-2.5-flash')).toEqual(['text', 'image'])
  })
})
