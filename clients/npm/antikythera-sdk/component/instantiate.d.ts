import type { RuntimeHooks } from '../runtime/control';
import type * as Runner from './interfaces/antikythera-agent-sdk-runner';

/** Instantiate a private jco Component Model runner with instance-owned imports. */
export function createRunner(hooks?: RuntimeHooks): Promise<typeof Runner>;
