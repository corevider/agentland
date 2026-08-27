import { defineConfig } from "vitest/config";
import { fileURLToPath } from "node:url";

export default defineConfig({
    test: {
        setupFiles: ["./vitest.setup.ts"],
        include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
        exclude: ["**/node_modules/**", "src-tauri/**"],
    },
    resolve: {
        alias: {
            "@": fileURLToPath(new URL("./src", import.meta.url)),
        },
    },
});
