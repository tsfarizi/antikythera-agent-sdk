'use strict';

/**
 * antikythera-agent/plugin — Plugin surface (optional, wasm-heavy).
 *
 * Re-export shim for the new namespace. Contains the WASM runtime and
 * orchestration that are optional / heavy.
 */

const { createAgentRuntime } = require('../runtime/index.js');
const { Orchestrator } = require('../orchestrator.js');

module.exports = {
  createAgentRuntime,
  Orchestrator,
};
