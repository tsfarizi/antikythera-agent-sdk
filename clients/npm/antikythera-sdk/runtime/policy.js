'use strict';

/**
 * Client-side permission gate (R4): default-deny. Only tool names listed in
 * `policy.allow` may execute locally. Every denial surfaces as an error whose
 * message starts with `permission:` — there is no silent degradation.
 */

/**
 * @param {{ allow?: Array<string> }} [policy]
 * @returns {{ check: (toolName: string) => void, allows: (toolName: string) => boolean }}
 */
function createPolicyGate(policy = {}) {
  const allow = new Set(Array.isArray(policy.allow) ? policy.allow : []);
  return {
    allow,
    check(toolName) {
      if (!allow.has(toolName)) {
        throw new Error(`permission: tool '${toolName}' not in allowlist`);
      }
    },
    allows(toolName) {
      return allow.has(toolName);
    },
    allowTool(_owner, toolName) {
      const name = typeof _owner === 'string' && typeof toolName === 'string' ? toolName : _owner;
      if (typeof name === 'string' && name) allow.add(name);
    },
    allow_tool(destination, toolName) {
      if (typeof toolName === 'string' && toolName) allow.add(toolName);
    },
  };
}

module.exports = { createPolicyGate };
