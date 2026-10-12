/** Compile the public TypeScript API used by server applications. */
import { createAgentDeployment } from "../../clients/npm/antikythera-sdk";
import type {
  DeliveryManifest,
  ConfiguredAgentRuntime,
  DeploymentConfig,
} from "../../clients/npm/antikythera-sdk/delivery";
import type {
  ClientEnvironment,
  DeliveredAgentRuntime,
} from "../../clients/npm/antikythera-sdk/delivery/client";
import { defineTool } from "../../clients/npm/antikythera-sdk/runtime/tools";

export const clientEnvironment: ClientEnvironment = {
  provider: async (request) => ({ content: request.model }),
};
export const useDeliveredRuntime = (runtime: DeliveredAgentRuntime) =>
  runtime.run("hello");

const config: DeploymentConfig = {
  id: "assistant",
  provider: "local",
  model: "configured-model",
  maxSteps: 4,
};
const existingTool = defineTool(
  { name: "reused", description: "Reuse a configured handler" },
  (args) => args,
);
export const reusedDeployment = createAgentDeployment(config, {
  tools: [{ tool: existingTool, allow: true }],
});
const deployment = createAgentDeployment(config, {
  provider: async (request, { signal }) => {
    signal?.throwIfAborted();
    const messages: unknown = JSON.parse(request.messages_json);
    return {
      content: JSON.stringify({ action: "final", content: String(messages) }),
    };
  },
  tools: [
    {
      definition: { name: "receipt", description: "Return a receipt" },
      handler: async (input) => input,
      allow: true,
    },
  ],
});

/** Verifies returned lifecycle and artifact types without executing the compile-only example. */
export async function checkDeliveryTypes() {
  const runtime: ConfiguredAgentRuntime = await deployment.createRuntime();
  const result = await runtime.run("hello");
  const output: string = result.output;
  runtime.close();
  const manifest: DeliveryManifest = await deployment.exportClient("./output", {
    serverUrl: "http://127.0.0.1:8000",
    bindingsDir: "./bindings",
    clientTools: { receipt: "receipt.mjs" },
  });
  const host = await deployment.serve({
    bind: "127.0.0.1",
    port: 0,
    allowedOrigins: ["https://app.example"],
  });
  await host.close();
  return { output, component: manifest.component, url: host.clientUrl };
}
