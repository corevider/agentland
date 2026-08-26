# Agentland

An open-source desktop workspace where a named crew of CLI coding agents works in parallel across
real git worktrees — each agent with its own branch, its own running dev server, and a preview beside
the diff.


Status: **M3.5 in progress.** M0 passed and Tauri is confirmed by measurement; M1 shipped worktrees, ports and per-worktree dev servers; M2 hires agents and runs their engines; the board now carries a card from assignment to a diff.

## Why M0 comes first

This product's normal state is eight agents streaming terminal output at once, with a WebGL island
rendering beside them. Tauri uses the system webview, which on Linux means WebKitGTK, not Chromium —
and xterm.js cannot count on the WebGL renderer there. If WebKitGTK cannot hold that load, the answer
is Electron with the same Rust core, and it is better to know that in week two than in month six.

So the first thing in the repository is a benchmark, not a feature.

### The gate

| Metric | Target |
| --- | --- |
| Panes | 8 concurrent |
| Output per pane | 10,000 lines/sec |
| Frame rate | ≥ 55 fps sustained |
| Worst frame | ≤ 32 ms |
| Dropped frames | 0 from the core, near 0 in the UI |

Anything below 30 fps fails the gate. Between 30 and 55 is marginal and needs a second look at the
frame budget before Tauri is confirmed.

### Result — the gate passed, Tauri is confirmed

Measured on this machine (GTX 1050 Ti, Ubuntu), 8 panes at 10,000 lines/sec each, steady state:

| Surface | Renderer | fps median | fps min | Worst frame | MB/s | Core drops |
| --- | --- | --- | --- | --- | --- | --- |
| **Tauri / WebKitGTK** | canvas | **62** | 57 | 20 ms median, 38 ms max | 8.07 | 0 |
| Firefox | canvas | 60 | 57 | 17 ms median, 40 ms max | ~8 | 0 |
| Firefox, before the render fix | webgl | 23 | 12 | 84 ms median, 116 ms max | 8.36 | 0 |

WebKitGTK did not lag the browser — it edged ahead of it. **The decision is Tauri.**

The third row is the same machine before the rendering strategy changed, and it is the more
interesting number: the first attempt managed 23 fps while the core dropped nothing and the UI
dropped 4,577 frames. The bottleneck was never the transport. It was asking xterm to parse 80,000
lines per second across eight panes — work no human can read and no renderer should attempt. Once
panes batched to one write per animation frame, unfocused panes throttled to 250 ms, and overloaded
panes collapsed to their last 48 KB, the same hardware tripled its frame rate.

Both passing runs fell back to the canvas renderer rather than WebGL, so 62 fps is what this costs
*without* GPU acceleration — there is headroom left on the table.

The island's requirement is answered too: the Tauri webview reports **WebGL2 with 16 available
contexts**, and the island needs one. What remains unmeasured is the two running together, which the
benchmark will cover once the island lands.

Why xterm's WebGL addon declines a context the webview clearly has is still open. The failure reason
is now captured in the pane label and in every sample instead of being swallowed, so the next run
answers it.

## Layout

```
crates/core/            Rust core: pty runtime, repositories, local API — no UI dependency
  src/pty.rs            pty spawn, output coalescing, replay buffer, session logs
  src/repo.rs           repository registry, worktree lifecycle, git via argument arrays
  src/ports.rs          port allocation from a workspace range
  src/bench.rs          synthetic load generator for the gate
  src/server.rs         axum HTTP + WebSocket on 127.0.0.1, token and Host guard
  src/bin/              standalone core, so the benchmark runs without Tauri
  tests/                worktree lifecycle against a real git repository
apps/desktop/
  src-tauri/            Tauri v2 shell; starts the core in-process
  src/                  Vite + React UI: xterm panes, benchmark HUD, repository panel
```

## M1 — the repository layer

**Worktrees never touch your clone's directory.** Point Agentland at any plain checkout; it registers
the path as-is and creates worktrees under `data/worktrees/<repo>/<name>`. No `main/` subdirectory to
prepare, no folders rearranged — the lesson taken from a tool that demands the
layout up front.

Each worktree gets a branch (`agent/<name>`) and a port from the 4100–4999 range, allocated by
probing for a free socket, recorded in `data/repositories.json`, and released on teardown. Two
agents cannot land on the same port, and the port belongs to the worktree rather than to whoever
started first.

**Removal refuses to lose work.** A worktree with uncommitted changes returns
`400 … has 1 uncommitted file(s); pass force to discard them`. Only an explicit force discards it.

```bash
cargo test -p agentland-core
```

The tests build a real git repository in a temp directory, create two worktrees, check that they get
different ports, dirty one, assert the removal is refused, force it, and confirm the port is released
and the state survives a reload. A second suite parses every remote URL form a developer actually
uses — https, scp-style ssh, `ssh://` with nested GitLab groups, self-hosted hosts, and filesystem
paths — because a repository can come from GitHub, GitLab, a private server, or no server at all.

**Every remote is recorded, not just origin.** Host, owner and provider are parsed from each, which
is what a pull-request action will need later. A local checkout with no remote works exactly the
same. Registering a worktree as if it were a repository is refused, with the main checkout's path in
the message.

### Dev servers

A worktree's service is detected from its own files — `package.json` (Vite gets `--port` and
`--strictPort`, otherwise `PORT` is exported) or `Cargo.toml` — and started as a real pty in that
worktree, on that worktree's port. The core polls the port until it answers, then health-checks every
five seconds, so a dead server becomes a visible `unreachable` state instead of a blank page.

Verified end to end: register → worktree on port 4100 → `package.json (dev script)` detected →
`starting` → `ready` in two seconds → `curl :4100` answers.

## M2 — the crew

An agent is a record, not a terminal: a name, a role, an engine, and a worktree it owns. Engines are
detected by asking each known CLI for its version, so the hire form only offers what is actually on
this machine. Starting an agent spawns its engine as a pty inside its own worktree, with `--continue`
or the engine's equivalent when resuming.

Verified with Claude Code: hire → start → the engine opens in
`data/worktrees/<repo>/work1` and its output is captured to `sessions/<id>.log`. Hiring against a
missing engine or an unknown worktree is refused with the reason.

## Running it

### Browser (works today, no system dependencies)

Chromium is the baseline you compare WebKitGTK against, so this is a useful run in its own right.

```bash
cargo run -p agentland-core --bin agentland-core     # prints a token and a browser URL
cd apps/desktop && npm install && npm run dev
```

Open the URL the core printed — it carries the port and token. Pick 8 panes and 10,000 lines/sec,
press **run benchmark**, and read the HUD: fps, worst frame, MB/s, dropped frames.

### Tauri window (the actual gate)

Needs the WebKitGTK development libraries once, on this machine only:

```bash
./scripts/setup-linux.sh     # installs what is missing, then the Rust toolchain if absent
cd apps/desktop && npm run tauri dev
```

Same benchmark, same HUD, now inside the webview that ships to users. The difference between the two
runs is the decision.

### What users install (nothing)

The `-dev` packages above are build-time only. Nobody who downloads Agentland installs them:

- **`.deb` / `.rpm`** — Tauri generates the runtime `Depends:` list (`libwebkit2gtk-4.1-0`,
  `libgtk-3-0`, and friends) into the package metadata, so `apt install ./agentland.deb` pulls them
  automatically. On a normal desktop they are already present.
- **AppImage** — the required libraries are bundled inside the image. Download, `chmod +x`, run.
- **macOS and Windows** — WKWebView and WebView2 ship with the OS; WebView2 has a bootstrapper for
  the rare machine without it.

CI installs the same list as `setup-linux.sh` before building, so release artifacts never depend on
a hand-prepared machine.

## Design decisions already made

**Framing over streaming.** A pty read that returns 200 bytes must not become a 200-byte message.
Output is coalesced into 8 ms frames, capped at 32 KB, and flushed early when a burst drains — so
interactive typing stays instant while a build log arrives in ~125 frames per second instead of
thousands of tiny writes.

**WebSocket, not Tauri IPC.** Tauri's event IPC serializes through JSON, which is the wrong shape for
megabytes of terminal bytes. The core exposes a local WebSocket and sends binary frames; the webview
writes the `Uint8Array` straight into xterm.

**Backpressure with a visible cost.** The broadcast channel is bounded. When a subscriber falls
behind, the core sends a `dropped` notice instead of buffering forever, and the UI drops frames after
8 outstanding writes and counts them. A terminal that silently lies about what it showed is worse
than one that admits it skipped.

**Replay buffer.** Each session keeps its last 256 KB, replayed on connect, so opening a pane after
an agent has been working shows what happened instead of an empty screen.

**Security on day one**, because retrofitting it is how a project ended up with 297
unauthenticated endpoints that can spawn shells:

- binds `127.0.0.1` only
- token required on every request and every WebSocket, generated per run
- `Host` header allowlist on both HTTP and upgrade paths, closing DNS rebinding
- everything bundled, nothing loaded from a CDN

## Stack

- **Shell:** Tauri v2 — target under 30 MB against Electron, which starts around 150 MB
- **Core:** Rust — axum, `portable-pty` (WezTerm's pty layer), `tokio`; `git2`, `sqlx` and `keyring` to come
- **UI:** Vite + React + Tailwind + shadcn/ui, xterm.js with the WebGL addon and a canvas fallback
- **Island:** react-three-fiber, once the gate is green

Tauri is the shell, not the UI framework: it owns the window, the Rust core and the system webview,
and something still has to author the interface inside it. That is Vite + React — not Next.js, which
only earns its complexity when a server runs behind it, and a Tauri app's static build has none.

No Node sidecar either. A Tauri shell wrapping a Node server ships the Node runtime anyway, loses the
size win, and keeps the webview inconsistency. Either the core is Rust or the shell is Electron.

## After the gate

From the PRD, in order: repo layer (worktrees, ports) → crew → board and review → parallel preview →
island → v0.1. Roughly 27 weeks to v1.0 for one developer with agent assistance.


## M3 — board and review

A card is not a note. It carries a repository, and once assigned it carries an agent, a worktree, a
branch and an evidence trail.

**Assigning starts work.** Dropping a card on an agent records the assignment, moves the card to
*working*, and launches that agent's engine in its worktree with the card's title and brief as the
opening prompt. Engines take a prompt differently — Claude Code and Codex positionally, Gemini behind
`-p` — so the catalog carries a prompt style per engine rather than assuming one shape.

**Review reads what is actually there.** `git diff` alone would report an empty review for an agent
that only created new files, which is most agents on a first task. The review therefore lists
untracked files and renders a real patch for each, on top of the committed range and the working
tree, and counts them in the totals.

**Pull requests degrade honestly.** With a GitHub remote and `gh` installed the branch is pushed and
the PR opened, and its URL is attached to the card as evidence. Otherwise the branch is still pushed
and a compare URL is returned for GitHub or GitLab. With no remote at all, the answer is a plain
sentence rather than a stack trace: *this repository has no remote to open a pull request against.*

Verified end to end: create card → assign to Ada → engine starts in `work1` with the brief → new file
appears in the review with its patch → commit → the review switches to the committed range with the
commit listed.


## M3.5 — the island

The app opens on a low-poly island built from primitives, not model files: terraces, palms, a jetty
and a lighthouse, all generated from the roster. **Island form is a pure function of the crew** — no
progression state to save or lose. One to three agents make a sandbar; four to six a beach and palm
grove; seven to ten a forest and ridge; eleven or more a settlement with a harbour and the lighthouse
that will be X's post.

Each agent occupies a station whose *shape* carries its role — a workbench for an implementer, a
watchtower for a reviewer, an antenna for a researcher, a crane for ops — and a lamp whose colour
carries its state. A working agent's chimney smokes; nothing else animates, because **nothing on the
island moves unless a real process is doing something**.

**Cards are dropped onto stations.** The unassigned column sits on the left; dragging a card over the
island raycasts through the scene to find the station under the pointer, highlights it, and on drop
assigns the task — the same call the board makes, so the agent starts with the card as its brief.

**The island yields to the terminals.** The scene renders on demand rather than in a loop: 30 fps
while it is the active view, 5 fps in the background, and nothing at all while the window is hidden.
Its bundle is code-split, so the 900 KB of three.js loads when the island is opened rather than at
startup. If a webview grants no WebGL context, the island degrades to a list carrying the same
states instead of a blank canvas.


## X — the manager

X stands at the lighthouse and hands out work. It is deliberately not a black box: **every decision
carries one line of reasoning, recorded on the card as evidence.** A manager that cannot explain a
choice is a random number generator with a hat.

The policy is deterministic and tested, not a model guessing:

- an agent must be hired on the task's repository, or X refuses and says so
- the task's words are matched against roles — "review the auth changes" goes to a reviewer
- concurrency is capped per repository and per engine, so a runaway X cannot open twelve sessions
  against one rate limit
- when nobody is free, the card is queued with the reason attached rather than silently dropped
- **pausing X freezes new handouts and leaves running agents alone** — the control you want at 2am

Verified: two agents hired, a card reading "review the auth changes" dropped on X →
`assign · Rex is free and the task reads like reviewer work`, recorded on the card. Paused →
`queue · X is paused; nothing new is being handed out`, card in the queue.

On the island, the lighthouse is the drop target for X; its lamp goes dark when X is paused.


## Pane telemetry

Other tools put an activity line in every pane, and it is the cheapest way to tell a working
agent from a stuck one. Each session now carries its own statistics — start time, time of last
output, bytes and lines produced, and whether the child process is still alive — and the pane header
reads:

```
pane-6a8e-2   working 3s   1.2 MB   live · canvas
pane-6a8e-3   waiting 4m 12s   11 B   250ms · canvas
```

*working* means output arrived in the last two seconds; *waiting* is the time since it last said
anything; *exited* means the process is gone. The byte figure carries the line count in its tooltip.

**The context meter is deliberately empty.** Other tools show a token or context number per
pane, and the field exists here — but it stays `null` until a parser is verified against a real
engine session, because a meter that disagrees with the engine's own `/status` is worse than no
meter. Claude Code reports context inside a redrawing TUI; guessing at that with a regex would
produce a number that looks authoritative and is wrong.
