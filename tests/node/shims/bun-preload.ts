import { plugin } from "bun";

plugin({
  name: "node-test-shim-rewrite",
  setup(build) {
    build.onLoad({ filter: /\.test\.mjs$/ }, async (args) => {
      const text = await Bun.file(args.path).text();
      const contents = text.replace(/from\s+['"]node:test['"]/g, "from './shims/node-test.ts'");
      return { contents, loader: "js" };
    });
  }
});
