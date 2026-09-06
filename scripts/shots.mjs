#!/usr/bin/env node

/// Take the pictures the README shows, from a machine nobody has to trust.
///
/// A screenshot of the app as it actually sits on somebody's desk carries their
/// repository names, their cards, their notes and the paths on their disk — and
/// it goes stale the first time a panel is redrawn. So the pictures are made:
/// a scratch core on its own port, its own data directory and its own vault,
/// seeded with a crew and a board that exist nowhere else, driven through the
/// built interface in a headless browser, and torn down afterwards. Run it again
/// after a change to the interface and the README is current.
///
///     node scripts/shots.mjs
///
/// Nothing it touches belongs to anyone: the ports are deliberately not the
/// ones a running Agentland uses, and the repositories it photographs are made
/// in a temporary folder a moment earlier.

import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const shelf = join(root, "docs");

// Away from anything a person has running: 9470 is the app's core, 5273 its
// interface, and 9222 is a browser somebody is reading this in.
const PORT = Number(process.env.SHOTS_CORE_PORT ?? 9491);
const SITE = Number(process.env.SHOTS_SITE_PORT ?? 5274);
const CHROME = Number(process.env.SHOTS_CHROME_PORT ?? 9339);
const TOKEN = "shots";
const WIDTH = 1500;
const HEIGHT = 940;

const rest = (ms) => new Promise((go) => setTimeout(go, ms));
const say = (words) => console.log(`  ${words}`);

async function ask(path, method = "GET", body) {
    const answer = await fetch(`http://127.0.0.1:${PORT}${path}`, {
        method,
        headers: { "x-auth-token": TOKEN, "content-type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
    });

    if (!answer.ok) {
        throw new Error(`${method} ${path} → ${answer.status} ${await answer.text()}`);
    }

    // Not every route answers with a document — a command that was accepted
    // says so with an empty body, and asking JSON to parse nothing throws
    // somewhere far from the call that did it.
    const said = await answer.text();
    return said.trim() === "" ? null : JSON.parse(said);
}

async function until(what, tries = 60) {
    for (let attempt = 0; attempt < tries; attempt += 1) {
        try {
            return await what();
        } catch {
            await rest(500);
        }
    }
    throw new Error("waited and it never came up");
}

/// A repository that is a repository: worktrees are git worktrees, and the app
/// refuses a folder that only looks like one.
function a_repository(where, name, readme) {
    const path = join(where, name);
    mkdirSync(path, { recursive: true });
    const git = (...args) => execFileSync("git", args, { cwd: path, stdio: "ignore" });

    git("init", "-q", "-b", "main");
    git("config", "user.email", "crew@example.com");
    git("config", "user.name", "the crew");
    git("config", "commit.gpgsign", "false");
    writeFileSync(join(path, "README.md"), readme);
    git("add", "-A");
    git("commit", "-qm", "first");

    return path;
}

async function seed(where) {
    const api = a_repository(where, "orchard-api", "# orchard-api\n\nThe service the shop runs on.\n");
    const web = a_repository(where, "orchard-web", "# orchard-web\n\nThe storefront.\n");

    const [first, second] = await Promise.all([
        ask("/repos", "POST", { path: api }),
        ask("/repos", "POST", { path: web }),
    ]);

    for (const [repo, trees] of [
        [first.id, ["ada-desk", "kai-desk"]],
        [second.id, ["sandbar"]],
    ]) {
        for (const name of trees) {
            await ask(`/repos/${repo}/worktrees`, "POST", { name });
        }
    }

    const workspace = await ask("/workspaces", "POST", {
        name: "Orchard",
        repository_ids: [first.id, second.id],
    });
    await ask("/workspaces/active", "POST", { id: workspace.id });

    const crew = [
        { name: "Ada", role: "commander", worktree: "ada-desk", repository_id: first.id },
        { name: "Kai", role: "builder", worktree: "kai-desk", repository_id: first.id },
        { name: "Wren", role: "builder", worktree: "sandbar", repository_id: second.id },
    ];

    const hired = [];
    for (const one of crew) {
        hired.push(await ask("/agents", "POST", { ...one, engine_id: "claude", model: "opus" }));
    }

    // Nobody is handed a card here. Handing one over is what starts an agent,
    // and starting an agent starts a real engine on whoever runs this — a
    // picture is not worth anybody's tokens. So the board is set the way a
    // person sets one: cards moved to where the work is.
    const cards = [
        { title: "Rate-limit the checkout endpoint", repository_id: first.id },
        { title: "Cart totals drop the currency on refunds", repository_id: first.id },
        { title: "Move the storefront to the new type-scale", repository_id: second.id },
        { title: "Give /health something worth reading", repository_id: first.id, column: "review" },
        { title: "Retire the legacy price table", repository_id: first.id, column: "done" },
    ];

    for (const card of cards) {
        const made = await ask("/tasks", "POST", {
            title: card.title,
            body: "",
            repository_id: card.repository_id,
        });

        if (card.column) {
            await ask(`/tasks/${made.id}/move`, "POST", { column: card.column });
        }
    }

    const notes = [
        {
            title: "The port contract",
            body: "Every worktree gets one port, handed to the dev server as PORT. Nothing reads a port from a file, and nothing hard-codes one — see [[Worktree ports]] for where the number comes from.",
            tags: ["ports", "contract"],
            scope: "shared",
        },
        {
            title: "Worktree ports",
            body: "A worktree is allocated a port when it is created and gives it back when it is removed. The allocation survives a restart, so a preview link keeps working.",
            tags: ["ports"],
            scope: "shared",
        },
        {
            title: "How we review a diff",
            body: "Read the test before the change. A card leaves review when somebody can say what would have caught the bug, not when the diff looks tidy. The port rules are in [[The port contract]].",
            tags: ["review"],
            scope: "shared",
        },
    ];

    for (const note of notes) {
        await ask("/notes", "POST", { ...note, written_by: "ada" });
    }

    const memories = [
        { text: "The checkout service reads PORT from the environment, never a config file.", approved: true },
        { text: "Refund totals are cents, not decimals — the currency lives beside them.", approved: true },
        { text: "The storefront's type-scale is set in one file; changing it anywhere else is a bug.", approved: true },
        { text: "The staging database is restored nightly, so a migration tried there is not lost work.", approved: false },
    ];

    for (const memory of memories) {
        const written = await ask("/memories", "POST", {
            text: memory.text,
            scope: "shared",
            proposed_by: "kai",
        });

        if (memory.approved) {
            await ask("/memories/answer", "POST", { slug: written.id, approved: true });
        }
    }

    await ask("/notes", "POST", {
        title: "A note nothing points at",
        body: "Left here on purpose, so the check in the notes panel has something honest to say.",
        tags: ["scratch"],
        scope: "shared",
        written_by: "wren",
    });

    say(`seeded ${crew.length} agents, ${cards.length} cards, ${notes.length + 1} notes, ${memories.length} memories`);
}

/// The interface, driven the way a person drives it.
class Window {
    constructor(socket) {
        this.socket = socket;
        this.id = 0;
        this.waiting = new Map();
        socket.onmessage = (event) => {
            const message = JSON.parse(event.data);
            if (message.id && this.waiting.has(message.id)) {
                this.waiting.get(message.id)(message);
                this.waiting.delete(message.id);
            }
        };
    }

    static async open() {
        const tabs = await until(async () => {
            const found = await (await fetch(`http://127.0.0.1:${CHROME}/json`)).json();
            const tab = found.find((entry) => entry.url.includes(`:${SITE}`));
            if (!tab) throw new Error("no tab yet");
            return tab;
        });

        const socket = new WebSocket(tabs.webSocketDebuggerUrl);
        await new Promise((go) => (socket.onopen = go));
        const window = new Window(socket);
        await window.send("Page.enable");
        return window;
    }

    send(method, params = {}) {
        return new Promise((go) => {
            const mine = (this.id += 1);
            this.waiting.set(mine, go);
            this.socket.send(JSON.stringify({ id: mine, method, params }));
        });
    }

    async evaluate(code) {
        const answer = await this.send("Runtime.evaluate", {
            expression: code,
            awaitPromise: true,
            returnByValue: true,
        });
        return answer.result?.result?.value;
    }

    async click(label, tries = 20) {
        for (let attempt = 0; attempt < tries; attempt += 1) {
            if (await this.press(label)) {
                return;
            }
            await rest(500);
        }

        throw new Error(`nothing on the screen says "${label}"`);
    }

    async press(label) {
        return this.evaluate(`
            (() => {
                const wanted = [...document.querySelectorAll("button")]
                    .find((button) => button.textContent.trim() === ${JSON.stringify(label)});
                if (!wanted) return false;
                wanted.click();
                return true;
            })()
        `);
    }

    async shoot(name) {
        const shot = await this.send("Page.captureScreenshot", { format: "png" });
        const path = join(shelf, `${name}.png`);
        writeFileSync(path, Buffer.from(shot.result.data, "base64"));
        say(`docs/${name}.png`);
    }

    close() {
        this.socket.close();
    }
}

async function show(view) {
    await ask("/ui/commands", "POST", { name: `only:${view}` });
    await rest(1800);
}

const running = [];
function start(command, args, options = {}) {
    const child = spawn(command, args, { stdio: "ignore", ...options });
    running.push(child);
    return child;
}

function stop_everything() {
    for (const child of running) {
        try {
            child.kill();
        } catch {
            // It is already gone, which is what we wanted.
        }
    }
}

const scratch = mkdtempSync(join(tmpdir(), "agentland-shots-"));

try {
    mkdirSync(shelf, { recursive: true });

    const core = join(root, "target", "debug", "agentland-core");
    if (!existsSync(core)) {
        console.log("building the core…");
        execFileSync("cargo", ["build", "-p", "agentland-core", "--bin", "agentland-core"], {
            cwd: root,
            stdio: "inherit",
        });
    }

    if (!existsSync(join(root, "apps/desktop/dist/index.html"))) {
        console.log("building the interface…");
        execFileSync("npx", ["vite", "build"], { cwd: join(root, "apps/desktop"), stdio: "inherit" });
    }

    console.log("a core of its own…");
    start(core, [], {
        cwd: root,
        env: {
            ...process.env,
            AGENTLAND_HOST: "127.0.0.1",
            AGENTLAND_PORT: String(PORT),
            AGENTLAND_TOKEN: TOKEN,
            AGENTLAND_DATA_DIR: join(scratch, "data"),
            // The panel shows this path, so it is worth it reading like a
            // notes folder rather than a temporary directory with a vault in it.
            AGENTLAND_VAULT_DIR: join(scratch, "Documents", "Agentland"),
            // The core answers a page only from an origin it was told about,
            // and the list it defaults to names the port a running Agentland
            // serves its interface on. This one is served somewhere else on
            // purpose, so it has to say so — without this the window loads and
            // every call in it fails with "Failed to fetch".
            AGENTLAND_ALLOWED_ORIGINS: `http://127.0.0.1:${SITE},http://localhost:${SITE}`,
        },
    });
    await until(() => ask("/repos"));

    // Held before there is anything for it to act on: a dispatcher that decides
    // to give somebody a card would start an engine to hand it over.
    await ask("/dispatch/pause", "POST", { paused: true });

    console.log("a crew that exists nowhere else…");
    await seed(scratch);

    console.log("the built interface…");
    start("npx", ["vite", "preview", "--port", String(SITE), "--strictPort"], {
        cwd: join(root, "apps/desktop"),
    });
    await until(async () => {
        const answer = await fetch(`http://127.0.0.1:${SITE}/`);
        if (!answer.ok) throw new Error("not serving yet");
        return true;
    });

    console.log("a browser nobody is reading in…");
    start("google-chrome", [
        "--headless=new",
        `--remote-debugging-port=${CHROME}`,
        "--disable-extensions",
        "--disable-gpu",
        `--user-data-dir=${join(scratch, "chrome")}`,
        `--window-size=${WIDTH},${HEIGHT}`,
        `http://127.0.0.1:${SITE}/?port=${PORT}&token=${TOKEN}`,
    ]);

    const window = await Window.open();
    await rest(4000);

    console.log("pictures…");

    // The island draws an agent once its pane is running, and starting a pane
    // starts a real engine. The crew list says who has been hired without
    // spending anybody's tokens to say it.
    await show("crew");
    await window.shoot("the-crew");

    await show("board");
    await window.shoot("the-board");

    await show("memory");
    await window.shoot("what-the-crew-remembers");

    await show("notes");
    await window.click("check");
    await rest(2000);
    await window.shoot("checking-the-vault");

    window.close();
    console.log("done.");
} catch (trouble) {
    // Whatever went wrong is the thing worth reading. Letting the tidying up
    // throw over the top of it is how a failure ends up reported as "directory
    // not empty".
    console.error(trouble);
    process.exitCode = 1;
} finally {
    stop_everything();
    // A browser that has just been asked to stop is still writing to its
    // profile for a moment after.
    await rest(1000);
    rmSync(scratch, { recursive: true, force: true, maxRetries: 8, retryDelay: 250 });
}
