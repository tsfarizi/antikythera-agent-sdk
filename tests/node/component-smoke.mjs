/**
 * CI smoke for the legacy jco namespace retained alongside the isolated factory.
 */
import { runner } from '../../clients/npm/antikythera-sdk/component/antikythera-sdk.js';

const sessionId = runner.init('{"session_id":"smoke-1"}');
if (sessionId !== 'smoke-1') {
  throw new Error('init did not return session_id "smoke-1", got: ' + JSON.stringify(sessionId));
}
const state = JSON.parse(runner.getState(sessionId));
if (typeof state !== 'object' || state === null) {
  throw new Error('getState did not return a JSON object');
}
console.log('NODE_COMPONENT_SMOKE_OK session=' + sessionId + ' state_keys=' + Object.keys(state).join(','));
