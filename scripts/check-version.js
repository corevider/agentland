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

/// Tauri refuses to build when a plugin's npm package and its Rust crate are on
/// different minor versions, and it only says so at build time. That is a fine
/// place to find out during development and a terrible one at a tag: the
/// release fails after the gate has passed, with the tag already pushed.
///
/// Measured — v0.1.0's first attempt died on `tauri-plugin-updater (v2.10.1) :
/// @tauri-apps/plugin-updater (v2.11.0)`, which nothing had checked because
/// adding an npm package does not touch Cargo.lock.
function plugin_pairs() {
    const lock = readFileSync(join(root, "Cargo.lock"), "utf8");
    const npm = JSON.parse(readFileSync(join(root, "apps/desktop/package-lock.json"), "utf8"));

    const crates = new Map();
    for (const block of lock.split("\n[[package]]")) {
        const name = block.match(/^\s*name = "(tauri-plugin-[a-z-]+)"/m);
        const version = block.match(/^\s*version = "([^"]+)"/m);
        if (name && version) {
            crates.set(name[1], version[1]);
        }
    }

    const trouble = [];
    for (const [crate, crate_version] of crates) {
        const package_name = `@tauri-apps/${crate.replace("tauri-", "")}`;
        const entry = npm.packages?.[`node_modules/${package_name}`];
        if (!entry) {
            continue; // A crate with no npm half has nothing to disagree with.
        }

        const same = (a, b) => a.split(".").slice(0, 2).join(".") === b.split(".").slice(0, 2).join(".");
        const mark = same(crate_version, entry.version) ? " " : "!";
        console.log(`${mark} ${crate.padEnd(28)} ${crate_version.padEnd(10)} ${package_name} ${entry.version}`);
        if (mark === "!") {
            trouble.push(`${crate} ${crate_version} against ${package_name} ${entry.version}`);
        }
    }

    return trouble;
}

const drifted = plugin_pairs();
if (drifted.length > 0) {
    console.error("\nA plugin's two halves are on different minor versions:");
    for (const line of drifted) {
        console.error(`  ${line}`);
    }
    console.error("Tauri refuses to build this, and it says so only at build time.");
    process.exit(1);
}

const unique = new Set(Object.values(versions));

for (const [file, version] of Object.entries(versions)) {
    console.log(`${file.padEnd(20)} ${version}`);
}

if (unique.size !== 1) {
    console.error("\nVersions disagree. Release artefacts would be mislabelled.");
    process.exit(1);
}

console.log(`\nConsistent at ${[...unique][0]}`);
