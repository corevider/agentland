#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function cargo_workspace_version() {
    const manifest = readFileSync(join(root, "Cargo.toml"), "utf8");
    const match = manifest.match(/\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"/);
    if (!match) {
        throw new Error("Cargo.toml has no workspace.package version");
    }
    return match[1];
}

function tauri_version() {
    const config = JSON.parse(readFileSync(join(root, "apps/desktop/src-tauri/tauri.conf.json"), "utf8"));
    return config.version;
}

const versions = {
    "Cargo.toml": cargo_workspace_version(),
    "tauri.conf.json": tauri_version(),
};

const unique = new Set(Object.values(versions));

for (const [file, version] of Object.entries(versions)) {
    console.log(`${file.padEnd(20)} ${version}`);
}

if (unique.size !== 1) {
    console.error("\nVersions disagree. Release artefacts would be mislabelled.");
    process.exit(1);
}

console.log(`\nConsistent at ${[...unique][0]}`);
