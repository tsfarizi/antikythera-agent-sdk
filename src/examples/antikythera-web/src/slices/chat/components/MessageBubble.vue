<script setup lang="ts">
/**
 * Render message markdown and expandable tool results with role-specific colors.
 * Tool arguments and results remain structured JSON until the details panel is opened.
 */
import { computed, ref } from 'vue'
import type { Message } from '../core/message'
import { renderMarkdown } from '@/shared/utils/markdown'

const props = defineProps<{
  message: Message
}>()

const toolsExpanded = ref(false)

const renderedContent = computed(() => {
  if (!props.message.content) return ''
  return renderMarkdown(props.message.content)
})

/** Format a tool payload as indented JSON, preserving an explicit null display. */
function formatJson(obj: unknown): string {
  if (obj === null || obj === undefined) return 'null'
  return JSON.stringify(obj, null, 2)
}

/** Replace underscores in tool identifiers with spaces for the visible label. */
function toolNameDisplay(tool: string): string {
  return tool.replace(/_/g, ' ')
}
</script>

<template>
  <div :class="['message', message.role]">
    <div v-if="message.toolCalls && message.toolCalls.length > 0" class="tool-calls">
      <button class="tool-toggle" @click="toolsExpanded = !toolsExpanded">
        <span class="tool-icon">⚙</span>
        <span class="tool-label">
          {{ message.toolCalls.length }} tool call{{ message.toolCalls.length > 1 ? 's' : '' }}
        </span>
        <span class="tool-chevron" :class="{ expanded: toolsExpanded }">▸</span>
      </button>
      <div v-if="toolsExpanded" class="tool-details">
        <div
          v-for="(tc, idx) in message.toolCalls"
          :key="idx"
          class="tool-entry"
          :class="{ 'tool-error': !tc.success }"
        >
          <div class="tool-header">
            <span class="tool-status">{{ tc.success ? '✓' : '✗' }}</span>
            <span class="tool-name">{{ toolNameDisplay(tc.tool) }}</span>
          </div>
          <div class="tool-io">
            <div class="tool-section">
              <span class="tool-section-label">Input</span>
              <pre class="tool-json">{{ formatJson(tc.input) }}</pre>
            </div>
            <div class="tool-section">
              <span class="tool-section-label">{{ tc.success ? 'Output' : 'Error' }}</span>
              <pre v-if="tc.success" class="tool-json">{{ formatJson(tc.output) }}</pre>
              <pre v-else class="tool-json tool-error-text">{{ tc.error || 'Unknown error' }}</pre>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div v-if="message.content" class="bubble markdown-body" v-html="renderedContent" />
  </div>
</template>

<style scoped>
.message {
  display: flex;
  flex-direction: column;
}

.message.user {
  align-items: flex-end;
}

.message.assistant {
  align-items: flex-start;
}

.bubble {
  max-width: 70%;
  padding: 12px 16px;
  border-radius: 16px;
  font-size: 15px;
  line-height: 1.5;
}

.message.user .bubble {
  background: #3b82f6;
  color: white;
  border-bottom-right-radius: 4px;
}

.message.assistant .bubble {
  background: #f3f4f6;
  color: #111;
  border-bottom-left-radius: 4px;
}

.markdown-body :deep(p) {
  margin: 0 0 8px 0;
}

.markdown-body :deep(p:last-child) {
  margin-bottom: 0;
}

.markdown-body :deep(strong) {
  font-weight: 700;
}

.markdown-body :deep(em) {
  font-style: italic;
}

.markdown-body :deep(code) {
  background: rgba(0, 0, 0, 0.06);
  padding: 2px 6px;
  border-radius: 4px;
  font-family: 'JetBrains Mono', 'Fira Code', monospace;
  font-size: 13px;
}

.markdown-body :deep(pre) {
  background: #1e1e2e;
  color: #cdd6f4;
  padding: 10px 14px;
  border-radius: 8px;
  overflow-x: auto;
  margin: 8px 0;
}

.markdown-body :deep(pre code) {
  background: none;
  padding: 0;
  color: inherit;
  font-size: 13px;
}

.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  margin: 4px 0;
  padding-left: 20px;
}

.markdown-body :deep(li) {
  margin: 2px 0;
}

.markdown-body :deep(blockquote) {
  border-left: 3px solid #d1d5db;
  padding-left: 12px;
  margin: 8px 0;
  color: #6b7280;
}

.markdown-body :deep(a) {
  color: #2563eb;
  text-decoration: underline;
}

.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid #e5e7eb;
  margin: 12px 0;
}

.markdown-body :deep(table) {
  border-collapse: collapse;
  margin: 8px 0;
  font-size: 14px;
}

.markdown-body :deep(th),
.markdown-body :deep(td) {
  border: 1px solid #d1d5db;
  padding: 6px 10px;
  text-align: left;
}

.markdown-body :deep(th) {
  background: #f9fafb;
  font-weight: 600;
}

.message.user .markdown-body :deep(code) {
  background: rgba(255, 255, 255, 0.2);
  color: white;
}

.message.user .markdown-body :deep(a) {
  color: #bfdbfe;
}

.message.user .markdown-body :deep(blockquote) {
  border-left-color: rgba(255, 255, 255, 0.4);
  color: rgba(255, 255, 255, 0.8);
}

.tool-calls {
  max-width: 70%;
  margin-bottom: 6px;
}

.tool-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  background: #f0f4ff;
  border: 1px solid #d0daf7;
  border-radius: 8px;
  cursor: pointer;
  font-size: 13px;
  color: #4b5563;
  transition: background 0.15s;
}

.tool-toggle:hover {
  background: #e5edff;
}

.tool-icon {
  font-size: 14px;
}

.tool-label {
  font-weight: 500;
}

.tool-chevron {
  transition: transform 0.2s;
  font-size: 12px;
}

.tool-chevron.expanded {
  transform: rotate(90deg);
}

.tool-details {
  margin-top: 4px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.tool-entry {
  background: #fafbfc;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  padding: 8px 12px;
  font-size: 13px;
}

.tool-entry.tool-error {
  border-color: #fca5a5;
  background: #fef2f2;
}

.tool-header {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
}

.tool-status {
  font-size: 12px;
}

.tool-entry:not(.tool-error) .tool-status {
  color: #22c55e;
}

.tool-error .tool-status {
  color: #dc2626;
}

.tool-name {
  font-weight: 600;
  color: #374151;
}

.tool-io {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.tool-section {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.tool-section-label {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  color: #9ca3af;
  letter-spacing: 0.5px;
}

.tool-json {
  background: #1e1e2e;
  color: #cdd6f4;
  padding: 6px 10px;
  border-radius: 6px;
  font-family: 'JetBrains Mono', 'Fira Code', monospace;
  font-size: 12px;
  line-height: 1.4;
  overflow-x: auto;
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
}

.tool-error-text {
  background: #fef2f2;
  color: #dc2626;
}
</style>
