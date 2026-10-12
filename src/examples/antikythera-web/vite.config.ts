import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [
    vue(),
  ],
  resolve: {
    alias: {
      "@": resolve(__dirname, "src"),
    },
  },
  server: {
    fs: {
      allow: [
        resolve(__dirname, ".."),
        /**
         * The `file:` dependency is a junction to ../../npm/antikythera-sdk; its real path must
         * be servable in dev (module + .wasm assets).
         */
        resolve(__dirname, "../../npm"),
      ],
    },
  },
  assetsInclude: ["**/*.wasm"],
  optimizeDeps: {
    exclude: ['antikythera-agent'],
  },
  /**
   * The jco-transpiled component module uses top-level await (ES2022); the default build target
   * (es2020) cannot emit it.
   */
  build: {
    target: 'es2022',
    /**
     * `antikythera-agent` is a `file:` junction to ../../npm/antikythera-sdk, so its modules
     * resolve to the real path OUTSIDE node_modules and the default CommonJS include
     * (/node_modules/) never converts the CJS runtime bridge. Extend the include to the realpath
     * so Rollup exposes the runtime's named exports (createAgentRuntime, ...).
     */
    commonjsOptions: {
      include: [/node_modules/, /[\\/]npm[\\/]antikythera-sdk[\\/]runtime[\\/]/],
    },
  },
});
