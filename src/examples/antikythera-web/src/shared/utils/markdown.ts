import { marked } from 'marked'

/**
 * Configure marked for chat output.
 */
marked.setOptions({
  breaks: true,
  gfm: true,
})

/**
 * Render markdown text to safe HTML.
 * Strips script tags and event handlers for safety.
 */
export function renderMarkdown(text: string): string {
  const raw = marked.parse(text) as string
  /**
   * Basic XSS prevention: strip script/iframe/object tags and on* handlers.
   */
  return raw
    .replace(/<script[\s\S]*?<\/script>/gi, '')
    .replace(/<iframe[\s\S]*?<\/iframe>/gi, '')
    .replace(/<object[\s\S]*?<\/object>/gi, '')
    .replace(/\son\w+\s*=\s*["'][^"']*["']/gi, '')
}
