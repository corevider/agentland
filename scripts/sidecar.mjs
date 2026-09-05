#!/usr/bin/env node

/// Put the crew's tool program where the installer will pick it up.
///
/// `agentland-mcp` is the program every agent talks to for the board, the
/// vault, memory and the rest. Running from source it sits beside the app in
/// `target/`, which is why it was never missed — but the installers shipped one
/// binary, so an installed Agentland handed its agents a command that was not on
/// the machine and every tool call failed at the handshake.
///
/// Tauri ships extra programs as `externalBin`, named for the target they were
/// built for and installed beside the app with the name stripped back. That is
/// exactly where `built_tool()` looks.

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const shelf = join(root, "apps/desktop/src-tauri/binaries");

function host_triple() {
    const spoken = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
    const host = spoken.match(/^host:\s*(\S+)$/m);
    if (!host) {
        throw new Error("rustc did not say what host it builds for");
    }
    return host[1];
}

/// What Tauri is building for. It says so in the environment of a build hook;
/// asking rustc is for a person running the script by hand.
function target_triple() {
    return process.env.TAURI_ENV_TARGET_TRIPLE?.trim() || host_triple();
}

/// A dev build is what `tauri dev` is about to run beside, and building the
/// sidecar in release there would be a minutes-long wait for a program that is
/// already sitting in `target/debug`.
const debug = process.env.TAURI_ENV_DEBUG === "true";
const triple = target_triple();
const suffix = triple.includes("windows") ? ".exe" : "";

// Naming the target when it is the one we are standing on would build every
// dependency a second time into a directory of its own, for the same bytes.
// It is named only when the build is genuinely for somewhere else.
const elsewhere = triple !== host_triple();

const args = ["build", "-p", "agentland-core", "--bin", "agentland-mcp"];
if (!debug) {
    args.push("--release");
}
if (elsewhere) {
    args.push("--target", triple);
}

execFileSync("cargo", args, { cwd: root, stdio: "inherit" });

const built = join(
    root,
    "target",
    ...(elsewhere ? [triple] : []),
    debug ? "debug" : "release",
    `agentland-mcp${suffix}`,
);

mkdirSync(shelf, { recursive: true });
const shipped = join(shelf, `agentland-mcp-${triple}${suffix}`);
copyFileSync(built, shipped);

console.log(`sidecar: ${shipped}`);
