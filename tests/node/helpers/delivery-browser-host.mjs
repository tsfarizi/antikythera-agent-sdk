/** Serves a real delivery export for browser acceptance; exits cleanly on stdin closure. */
import path from "node:path";
import { fileURLToPath } from "node:url";
import delivery from "../../../clients/npm/antikythera-sdk/delivery/index.js";

const deployment = delivery.createAgentDeployment(
  {
    id: "browser-agent",
    provider: "receipt",
    model: "configured-model",
    maxSteps: 4,
  },
  {
    provider: async (request) => ({
      content: JSON.parse(request.messages_json).some(
        (message) => message.role === "tool",
      )
        ? '{"action":"final","content":"delivered-7"}'
        : '{"action":"call_tool","tool":"delivery_receipt","input":{"value":7}}',
    }),
    tools: [
      {
        definition: {
          name: "delivery_receipt",
          description: "Create a numeric receipt",
        },
        handler: () => {
          throw new Error(
            "The server handler must not execute in this browser test",
          );
        },
        allow: true,
      },
    ],
  },
);
const host = await deployment.serve({
  port: Number(process.argv[2] ?? 0),
  bindingsDir: path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "../fixtures/delivery",
  ),
  clientTools: { delivery_receipt: "receipt.mjs" },
});
console.log(
  JSON.stringify({ url: `${host.url}/bundle/bindings/browser.html` }),
);
process.stdin.resume();
const stop = async () => {
  await host.close();
  process.exit(0);
};
process.stdin.on("end", stop);
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
