'use strict';

/**
 * antikythera-agent/core — Core surface (stable, no wasm).
 *
 * Re-export shim for the new namespace. Underlying modules remain in
 * `antikythera-agent` root; this package provides the `antikythera.core`
 * entry point per the namespace contract.
 */

const { PromptManager, SessionManager, getVersion } = require('../index.js');

module.exports = {
  PromptManager,
  SessionManager,
  getVersion,
};
