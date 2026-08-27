# Agentland

An open-source desktop workspace where a named crew of CLI coding agents works in parallel across
real git worktrees — each agent with its own branch, its own running dev server, and a preview beside
the diff.


Status: **M7 — approvals reach the phone.** 29 tests. M0 passed and Tauri is confirmed by measurement; M1 shipped worktrees, ports and per-worktree dev servers; M2 hires agents and runs their engines; the board now carries a card from assignment to a diff.

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
contexts**, and the island needs one.

### The island and the terminals together

That last question — what happens when the island renders while eight terminals stream — was left
open for months in the risk register. It is now measured. Both runs are the same benchmark, driven
through the command channel so the layout is the only difference:

| Layout | UI fps (median / min) | Worst frame | Throughput | Dropped | Island fps |
|---|---|---|---|---|---|
| 8 terminals only | 56 / 25 | 79 ms | 8.4 MB/s | 0 | — |
| island + 8 terminals | **60 / 29** | 69 ms | 8.2 MB/s | 0 | **29** |

The island costs nothing measurable, and it holds its governor exactly: the cap is 30 fps while
active and the measurement says 29. Throughput and dropped frames are unchanged.

Two honest caveats. The island takes half the window, so the terminals beside it render into a
smaller area — the load is not purely additive, which is part of why the island run scores *higher*.
And this run happened on a virtual display, because a Wayland window that is not on screen gets no
animation frames at all: the first attempt reported `fps=0`, which is the absence of a measurement
rather than a bad one. The renderer string the webview reports there is masked, so treat the fps as
the shape of the answer on this machine, not a hardware benchmark.

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


## The island

![The island at dusk, four humanoid agents on their platforms](docs/island.png)

The app opens here. The island is built from primitives — no model files, no asset pipeline — and its
**form is a pure function of the crew**, so there is no progression state to save or lose. One to
three agents make a sandbar; four to six a beach and palm grove; seven to ten a forest and ridge;
eleven or more a settlement with a harbour.

Each agent is a low-poly humanoid: panelled body, dark visor, and a **bulb above its head** carrying
its state. The tool beside it carries its role — a workbench for an implementer, a watchtower for a
reviewer, an antenna for a researcher, a crane for ops.

### Colour means one thing each

| Bulb | Meaning | The signal behind it |
| --- | --- | --- |
| 🟢 green | finished | its process exited after a run |
| 🟡 yellow | working | output arrived in the last ninety seconds |
| 🔴 red | **needs you** | it asked for approval, or it has been silent at a prompt |
| ⚪ grey | idle | never started |

Presence is computed from session statistics and the approval queue, never stored, and the reason
travels with it — the interface can always say *why* a light is red.

**Nothing moves unless a real process is doing something.** A working agent swings its arms; an agent
that needs you **raises its hand and waves**; a finished one stands still. That rule is what keeps the
island a status display rather than a screensaver.

### Clicking a robot opens it

A click on any agent opens a sheet beside the island: what it is, what state it is in and *why*, any
approval it is waiting on — answerable right there — a box that turns a sentence into a card and
hands it to that agent, buttons to start, resume, stop or open its terminal, and the last fourteen
lines it actually said, with the escape codes stripped.

Dragging to orbit does not select: a pointer that travelled more than six pixels is treated as a
drag, not a click.

### Cards are thrown, not filed

Dragging a card over the island raycasts to the station under the pointer and assigns on drop — the
same call the board makes. Drop it on the lighthouse instead and **X** takes it: the core records the
handoff as an event, and a lit shell arcs from the lighthouse to the chosen agent and flashes on
landing. The decision is visible, not just its result.

### Two things computed rather than guessed

The sky is a shader that colours by `normalize(worldPosition).y`, so the sunset band sits exactly on
the horizon from any camera angle. It replaced a canvas gradient on a sphere that took three failed
attempts, because the mapping from texture rows to world height was assumed rather than derived.

The raised hand failed the same way: rotating the arm about X lifted it *behind* the body, where the
camera never saw it. Rotating about Z sends the arm's `-Y` axis to `(sin θ, -cos θ)`, so 2.5 radians
lifts it up and outward.

Placement follows the same rule. Stations keep even spacing and the whole ring rotates to the offset
with the most clearance from the lighthouse and the jetty; palms are rejected within 1.15 units of
anything already placed; everything stands on the terrain height computed for its own distance from
the centre, rather than a fixed y that buried the robots' feet.

### It yields to the terminals

The scene renders on demand rather than in a loop: 30 fps while it is the active view, 5 fps in the
background, nothing while the window is hidden. Its bundle is code-split, so three.js loads when the
island opens rather than at startup. Without a WebGL context the island degrades to a list carrying
the same states, never a blank canvas.

**Capturing it:** GNOME refuses external screenshot calls on Wayland, so the app takes its own — the
context menu's *Capture the island*, or `POST /ui/commands {"name":"capture-island"}`, writes a PNG
from the canvas.

## The look

Warm dusk on the water, chosen against one constraint: people stare at this for hours next to a
terminal, so the summer feeling lives in colour and texture, never in contrast.

| Token | Where it lives |
| --- | --- |
| `lagoon-deep` · `lagoon` · `shallow` | grounds and surfaces |
| `reef` · `foam` | borders |
| `linen` · `driftwood` · `shell` · `shade` | text, four steps of it |
| `turquoise` | anything interactive |
| `sun` · `coral` · `palm` | working · needs you · finished |
| `teak` | wood on the island |

No hex codes remain in the components; the eight panels read from these tokens. Three typefaces do
three jobs: **Fraunces** for display, **Figtree** for the interface, **IBM Plex Mono** for data and
terminals — the terminal keeps its own typographic world rather than being dressed up as chat.

Fonts ship inside the bundle. The security section says nothing loads from a CDN, and a design
change is not a reason to break it.

### Right-click is ours

The webview's reload-and-inspect menu is suppressed and replaced with the app's own: the five views,
settings, capture, reload — and developer tools only in dev builds. The hook is context-aware, so a
robot or a card can carry its own commands later.

### Views stay mounted

Switching tabs used to unmount the previous view, which meant rebuilding the whole WebGL scene on
every visit to the island. Views are now hidden rather than destroyed, and each pauses its own
polling while hidden.

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


## M4 — v0.1

`npm run tauri build` produces both Linux bundles:

| Artefact | Size |   |
| --- | --- | --- |
| `Agentland_0.0.1_amd64.deb` | **4.2 MB** | — |
| `Agentland_0.0.1_amd64.AppImage` | **80 MB** | the whole runtime, in one file |
| `agentland-desktop` binary | 11 MB | — |

The `.deb` carries `Depends: libwebkit2gtk-4.1-0, libgtk-3-0`, generated into the package metadata,
which is the promise made earlier in this file: users install nothing by hand.

### Container mode

```bash
./scripts/container.sh
```

Builds an image with the core and the interface, runs it as uid 1000 with `cap_drop: ALL` and
`no-new-privileges`, publishes the port on loopback only, and mounts nothing but the projects
directory you name. The core serves the interface itself, so there is one process and one port.

Verified inside the container:

```
UI without a token        200   (the app bundle is not secret)
data route without token  401
data route with token     200
forged Host header        403
```

Only the bundle paths — `/`, `/index.html`, `/assets/*`, `/favicon.ico` — skip the token. Every route
that touches a repository, a session or an agent still requires it, and the Host allowlist applies to
all of them.

### Updates

The updater ships **off**. It refuses to check without both a configured endpoint
(`AGENTLAND_UPDATER_ENDPOINTS`) and a public key in `tauri.conf.json`, so an unsigned or
unverifiable update cannot install — it is refused rather than warned about.

```bash
./scripts/release/generate-updater-key.sh    # writes the private key outside the repository
```

CI (`.github/workflows/ci.yml`) checks that `Cargo.toml` and `tauri.conf.json` agree on the version,
runs the Rust tests and the interface build, then bundles for Linux and macOS with the signing key
read from repository secrets.


## M5 — MCP and delegation

Agents talk to Agentland through one MCP server, `agentland-mcp`, a separate binary that speaks
JSON-RPC over stdio and calls the core's HTTP API. It is deliberately a separate process: two
processes writing the same state files would race.

Eight tools, grouped by domain:

| Tool | What an agent does with it |
| --- | --- |
| `task_list` / `task_create` / `task_move` | keep work on the board instead of in its head |
| `crew_list` | see who else is on the crew and what they are doing |
| `crew_delegate` | hand a card to X and get the decision *and its reason* back |
| `repo_list` / `repo_worktrees` | find repositories, branches, ports, uncommitted counts |
| `repo_review` | read its own diff, including untracked files |

**Every worktree gets the tools automatically.** Creating a worktree writes a `.mcp.json` next to the
code, so an engine launched there finds the server without configuration. The token is *not* written
into it — the file references `${AGENTLAND_TOKEN}`, and the agent process receives that variable when
Agentland starts it. A credential never lands on disk inside a repository.

The file is appended to the repository's `info/exclude`, so git never offers to commit it. That
append is careful: it reads what is there, adds the line only if missing, and leaves every rule the
user already had. The first version overwrote the file, which would have deleted their excludes.

### Fan-out, verified

Four agents on one repository, caps at three per repository, four cards handed to X:

```
assign  ada      Ada is the free agent on agentland-svc-demo with the closest role
assign  worker2  Worker2 is the free agent on agentland-svc-demo with the closest role
queue            3 of 3 allowed agents are already working on agentland-svc-demo
queue            3 of 3 allowed agents are already working on agentland-svc-demo
```

Three engines running concurrently, the rest queued with the reason attached. Caps are adjustable at
runtime through `POST /dispatch/caps`.

Stopping an agent now also reaps it — the first version killed the process without waiting, leaving
zombies behind in a program whose whole purpose is spawning processes.


## M6 — gateway, routines, mail, memory

### Memory, gated by approval

An agent proposes; a human approves; only then does it reach another agent's brief.

**Secrets are masked before the text is ever stored**, not before it is displayed. Nineteen unit
tests cover the credential shapes that actually leak — `sk-`, `ghp_`, `github_pat_`, `AKIA`,
`xoxb-`, `glpat-`, `AIza`, JWTs, and long mixed-case tokens — plus assignments, where the variable
name survives and the value does not:

```
proposed : Deploy icin GITHUB_TOKEN=ghp_EXAMPLE_NOT_REAL kullaniyoruz
stored   : Deploy icin GITHUB_TOKEN=[redacted] kullaniyoruz
```

Ordinary prose is left alone — a masker that mangles normal sentences would be turned off within a
day.

### Agent mail

Messages between agents, with per-agent grants and one switch that stops all of it:

```
send while running   -> msg1 delivered
send while paused    -> {"error":"agent-to-agent messaging is paused"}
```

An inbox is delivered exactly once, and it arrives in the recipient's next brief rather than
interrupting a running session.

### Routines

A named agent, a brief, an interval. The ticker creates a card, hands it to the agent, and records
the outcome. **Two failures in a row disable the routine** instead of burning tokens nightly against
a broken assumption; a success clears the streak. `draft_only` appends *"Prepare the work and stop
before anything leaves this machine."*

Verified from a live run: `r1 last_result="card t9 handed to Ada"`, and the agent's command line
carried the draft-only sentence.

### The gateway

Credentials live in the OS keychain, or in an environment variable named after the integration when
no keychain is available — and **never in the engine's hands**. The agent calls `integration_call`;
Agentland makes the HTTP request and returns the result. The stored record carries service,
environment and *where the secret lives*, never the secret. Unsupported services are refused by name.

### Composing a brief

All three sources meet in one place, and every start path goes through it — assignment, X's dispatch,
and routines. The first version only wired the routine path, so an assigned agent silently got
neither its mail nor the crew's memory:

```
ikinci inceleme · diff'i oku
What this crew has learned: - Migration dosyalari db/migrations altinda
Messages waiting for you: - from ada: auth dali incelemeye hazir
```

The unapproved memory — the one holding a token — is absent, which is the whole point.

MCP grows to twelve tools: `crew_message`, `memory_propose`, `integration_list` and
`integration_call` join the eight from M5.


## M7 — the phone companion

### A phone cannot open a shell

The security property comes first, because a device carried outside the house is the one most likely
to be lost. Pairing issues a **second token with approve-only scope**, and the guard checks scope
against method and path before the route ever runs:

```
phone token can:            phone token cannot:
  read approvals    200       open a shell        403
  read the crew     200       list sessions       403
  approve / reject  200       remove a worktree   403
                              start an agent      403
                              call an integration 403
```

Four unit tests pin that matrix, including the case that matters most — an approve-only token must
never reach `/sessions`. Devices are listed and revoked from the desktop; revocation is immediate.

### Approvals

An agent asks with `request_approval` and carries on; a human answers from anywhere on the tailnet.
An approval is answered exactly once — a second answer is refused rather than flipping a decision
someone already acted on.

### The page

`apps/mobile` is a single HTML file with no build step, no framework and no three.js: pending
approvals with Approve and Reject, the crew with live states, and the board minus finished cards. It
installs as a PWA and stores its token locally after the first open, so the token leaves the URL.

### Reaching it

```bash
./scripts/pair-phone.sh "ege's phone"
```

The script refuses to continue without a tailnet address, because this is not a thing to expose to
the public internet. It prints the exact command to restart the core bound to the tailnet — with the
Host allowlist extended to that address and nothing else — and the URL to open on the phone.

### A regression this caught

The mobile and desktop static files were mounted **after** the guard layer, so neither carried the
Host check — a forged `Host` header fetched the app bundle. Routes are now assembled first and the
guard wraps the finished router, so `/mobile` answers 403 to a forged host exactly like every data
route does.

## Paying down what the PRD owed

Three debts were carried for months, each written down as a risk rather than fixed. They are closed.

### State moved to SQLite

Every store kept its whole state in one pretty-printed JSON file and rewrote it in full on each
change. That survives until it doesn't: a crash between `write` and `close` takes the file with it.
State now lives in one WAL-mode SQLite database. Each store keeps its struct; what changes is that a
write is atomic.

The migration mattered more than the schema. On first start a legacy `board.json` is imported and
renamed to `board.json.imported` — kept, not deleted, so a bad import can be undone by hand. A file
that will not parse is left exactly where it is and reported, because destroying an unreadable file
is the one unrecoverable move.

It was verified against a copy of the live directory: 4 agents, 12 cards, 2 repositories, 2 memories
and 1 routine came back through the API, and a card created afterwards was read straight out of the
database file.

That verification found a second bug. The data directory was hardcoded as `"data"` in nine places,
resolved against the working directory — so the desktop app's state lived wherever it happened to be
launched from, and a packaged build would have started empty. It is now `ServerConfig::data_dir`,
overridable with `AGENTLAND_DATA_DIR`, which is also what made the migration testable in isolation.

### Skills

Four built-in skills ship with the binary — systematic debugging, test-driven development, code
review, architecture diagrams. A skill is a folder with a `SKILL.md`: a `---` header naming it, and
instructions underneath. Users write their own into the data directory and they are read on start.
Built-ins cannot be overwritten or deleted.

Installing a skill on an agent puts it in that agent's opening brief, next to what the crew has
learned and its waiting mail. It is not a prompt the user pastes; it is content the product carries.

### The island and the terminals, measured

Recorded above — the island costs nothing measurable, and holds its 30 fps governor while eight
terminals stream at 10,000 lines a second.

## The workspace, deepened

Three fixed slots became four, and every slot holds a strip of tabs. Tabs drag between slots, close
individually, and an emptied slot stays as a drop target rather than vanishing. The **Preview**
panel lists the dev servers the service registry is running and frames the selected worktree's
localhost inside the app — the thing this product exists to do at eight-agent scale, finally visible
beside the work instead of in a separate browser.

A layout saved by the previous version is upgraded on load, and a panel that no longer exists is
dropped rather than leaving a slot rendering nothing. Eight tests cover that upgrade and the tab
arithmetic; the UI had no test runner before this, and now has vitest.

## Real work, end to end

The product was pointed at the only genuinely real codebase on hand: itself.

A card was written — *let a phone token read the skills library*, because the scope matrix in
`auth.rs` did not include `/skills`, so the phone could not show what a crew member knows. It was
assigned to an agent called Kai, running Claude Code in a worktree, with the test-driven-development
skill installed. Kai added `/skills` to the allowed GET paths and wrote a test that also asserts the
negatives: `POST /skills` denied, `GET /skills/tdd` denied. `cargo test` in the worktree went from 4
auth tests to 5, all passing.

Two things this run exposed that no unit test would have:

- **The engine asks questions before it works.** Claude Code opened with a trust prompt for the
  worktree and waited. An agent that is blocked on a question is not an agent that is working, and
  the terminal is where a person answers it.
- **There was no commit step.** The chain read card → diff → pull request, and the pull request
  pushed a branch that had no commits on it. `POST /repos/{id}/worktrees/{name}/commit` now stages
  and commits the worktree, and the pull request refuses to run before it: *commit the work first: 1
  file(s) in scope-skills are not committed*. A branch that is pushed to a remote with no web
  address is a success that says so, not an error.

The full chain then ran: card → assignment → real work → diff → commit `46d2414` → pushed to the
remote.

### The context meter, filled honestly

The field `context_percent` existed and nothing ever set it. The PRD's reason was sound: a number
that disagrees with the engine's own status would be worse than none.

Real output settled it. The running engine reports `Ctx: 44.5k` — tokens in context, not a
percentage, and it never states the window those tokens are a fraction of. Turning 44.5k into a
percentage would have required inventing that window, which is exactly the disagreeing number the
PRD warned about.

So the parser reports what the engine says and nothing more: a percentage when the engine prints a
percentage, a token count when it prints tokens, and nothing when it prints neither. The pane shows
`44.5k ctx` or `23% ctx left` accordingly.

The recorded session is committed as a test fixture, so the parser is tested against output a real
agent actually produced, escape sequences and the non-breaking space in `Ctx:\u{a0}44.5k` included —
a detail that would have broken a regex written from memory. Live, through the API, the meter read
`context_tokens=40400` from the running engine's own status line while `context_percent` stayed
null, which is the correct answer for this engine.

### What an agent knows, on the phone

The scope matrix let a phone read the crew but not the library, so the companion could say *Kai is
idle* and not *Kai knows how to work test-first*. An agent wrote that permission itself — the card
above — and the matrix now also admits `GET /agents/{id}/skills`, one agent at a time, GET only:
installing a skill is still a desktop action, and the test pins that down alongside the paths that
stay refused.

Tapping a crew card on the phone opens what that agent carries: each skill's name, when it is meant
to be used, and what it is for. An agent with none says so and points at where to give it one. The
list is fetched when the card is opened rather than on every five-second refresh, because a phone on
a tailnet should not pay for what nobody is looking at.

### The three the references still had

Held up against the two reference layouts again, three things were missing. Each one is a feature
underneath, not an icon.

**Terminals carry their own actions.** A card's header now has `+`, which opens another shell in the
same worktree — a session reports its working directory for that, which it did not before — and `⤢`,
which fills the panel with one terminal and drops back to the grid. Plus the close button that was
already there.

**A mode switch, meaning layout presets.** *Crew* puts the island and the board beside the
terminals; *Work* narrows to the board and gives the terminals the window; *Review* pairs the
repositories with the preview and keeps a terminal underneath. The switch highlights the preset you
are in, and stops highlighting the moment you drag a tab somewhere else, because then you are not in
it any more.

**Workspaces, which the data model always had.** Section 08 of the PRD specified a workspace that
groups repositories, and nothing had been built. Now the top bar carries the workspaces as tabs with
the number of agents in each, `All` for everything, and `+` to make one; clicking the active tab
opens its repository list. Choosing a workspace narrows the rail, the board and the terminals to its
repositories — the panes subtitle reads `1 of 2` rather than pretending the hidden one is not there.
A repository that is removed leaves every workspace that held it.

The reference layouts are still denser than this one, and that is the next thing to sit with rather
than guess at.

## The app was dying every forty minutes

It stopped twice on its own. Not a crash — a kill:

```
Unable to shrink memory footprint of process (4211 MB)
below the kill thresold (4096 MB). Killed
```

The webview was growing about 65 MB a minute while the app sat idle, so the watchdog reached it in
well under an hour. Six measurements, each cutting one suspect:

| What was measured | Growth |
|---|---|
| The island rendering | 66 → 64 MB/min without it. Not the island. |
| Panel count | One simple panel, still 66 MB/min. |
| Poll traffic | Every interval slowed fivefold: 66 → 59. Not the requests. |
| Compositing | `WEBKIT_DISABLE_COMPOSITING_MODE=1`, four minutes: 64 MB/min. |
| The same app in Chrome | Flat after warm-up. |
| A page with no script at all, same webview | **171 MB, flat for three minutes.** |

That last row killed my own hypothesis. I had said this looked like a WebKitGTK leak; the blank page
proved the engine holds still, so the growth was ours.

Chrome's console said the rest: **801 identical `THREE.WebGLShadowMap: PCFSoftShadowMap has been
deprecated` warnings in seventy seconds** — one per rendered frame, each retained with its stack.
R3F's bare `shadows` prop selects the deprecated soft shadow map; `shadows="percentage"` pins the
supported one and the warnings stop.

That was not the whole of it. With the warning gone the dev server still leaked, so the last
comparison was the one that mattered: the same app, the same island, the same webview, built rather
than served by Vite.

| Bundle | Memory over five minutes |
|---|---|
| Vite dev server | 425 → 990 MB, climbing |
| `vite build` output | 339 → 351 MB, flat |

**The shipped product does not have this leak; the dev server does.** It costs a restart every half
hour while developing and nothing to anyone who installs the app. `npm run dev:built` runs the shell
against a built bundle for long sessions.

The first measurement I trusted here was wrong twice — once blaming the engine, once comparing a
production build that could not reach the core and was therefore doing nothing (`TypeError: Load
failed` in the corner of the screenshot, which is what caught it). Both times the fix was another
measurement rather than another theory.

## A density pass

The reference layouts fit more on a screen than this one did, so the spacing was re-cut against
them. Nothing was redesigned; the same panels give back the room they were wasting.

- Native controls carry one base rule — 12px text, 3px/7px padding — instead of each call site
  choosing. The hire form went from two rows to one, and the board's title, brief and repository
  now share a row rather than stacking three deep.
- Panel padding dropped a step (`p-4` → `p-2.5`, `p-3` → `p-2`), tab strips and the frame gaps with
  it, and section headings lost a line of margin.
- The board's columns stopped being a five-way grid that squeezed cards until their buttons were
  clipped. They are a fixed 150px each in a strip that scrolls sideways, so a card stays readable
  when the panel is narrow.
- The island's unassigned column went from 288px to 208px, which is what it needs for a card.
- The crew rail lost two pixels a row, and the crew panel's action buttons went from `px-3 py-1` to
  `px-1.5 py-0.5`.

Measured on the same window: the board panel that showed one clipped card now shows the card, its
assignee control and its delete button; the crew panel that fit one agent row fits two and a half.

## Anything, anywhere

Four slots were still four slots, seven panels were a hardcoded union, and a panel could only exist
in one place at a time. Three limits, one shape underneath: the layout knew the panels by name.

**The layout is a tree.** A node is either a stack of tabs or a split of two nodes, with a fraction
between them. Every stack header carries `⊞` and `⊟`, which split it beside or below and drop a
panel into the new half; there is no ceiling on depth, and a test pushes it to fifteen stacks to say
so. When the last tab leaves a stack, the stack collapses and its sibling takes the room — except
for the final one, which stays empty so there is always somewhere to drop.

**Panels are a registry.** `workspace/registry.tsx` holds one entry per panel — id, label, hint, and
the component — and everything else reads from it: the rail, the tab strips, the add menu, what a
saved layout is allowed to restore. Adding a panel is one entry in that array. Panels take what they
need from a workspace context rather than a prop chain, so a new one does not touch `App.tsx` at
all.

**A panel can be open more than once.** A tab is an instance, not a panel name: two Preview panels
sit side by side on different ports, each with its own state, and closing one leaves the other. That
is what the wedge actually needs — eight agents means more than one running result to look at.

A layout saved by any earlier version still opens: the four-slot shape is converted, a panel that no
longer exists is dropped rather than left rendering nothing, and a stored value that is nonsense
falls back to the default. Fourteen tests cover the tree — splitting, moving, closing, collapsing,
the upgrade path and the clamped divider.

### The claim, tested

"Adding a panel is one entry in one array" is easy to write and easy to be wrong about, so a panel
was added to find out. Mail was the honest choice: M6 built it — messages, grants, a global pause —
and it had no interface at all, so an agent could be told something and nobody could see it.

Three files changed. `MailPanel.tsx`, new. `registry.tsx`, one import and one entry. `lib/core.ts`,
the four calls that reach `/mail` — an HTTP client for a new endpoint, which has nothing to do with
the layout.

Untouched: `App.tsx`, `Workspace.tsx`, `WorkspaceRail.tsx`, `layout.ts`, `presets.ts`. The panel
appeared in the rail's view list, in every stack's add menu, and answered `view:mail` on the command
channel without any of those files knowing it exists.

Then it was used rather than admired: a message typed into the panel and sent arrived in the core as
`msg1 ada → kai delivered=false`, and the pause button flipped the stored policy to
`{"paused":true}`. A registry test now guards the shape every entry has to have.

One thing the exercise cost: the registry test imports the panels, the panels import xterm, and
xterm's bundle wants `self` at import time. A three-line setup file gives the test runner that
global instead of pulling in a DOM implementation.

### Two more panels, and what they exposed

Memory and Routines were in the same state Mail had been in: built in M6, reachable over HTTP,
invisible. Both are panels now — memory proposes, approves, revokes and forgets with the scope it
belongs to and a mark when a secret was masked; routines create, enable, disable and delete, showing
when each last ran, whether it is due, and how many failures in a row it has taken.

Same three files each time: the panel, one entry in the registry, the calls in `lib/core.ts`.

Using them found two defects that reading would not have.

**The approve button returned a 400.** `POST /memories/{id}/approve` takes `{"approved": bool}` —
approve *or* reject — and the client sent no body at all. Fixed in the client, and the approved list
gained a `revoke` that sends `false`, which takes a memory out of the crew's brief without deleting
it.

**A plain start gave an agent nothing.** `start_agent` passed `None` where the brief belongs, so
memories, mail and skills only reached an agent that was started *by a card* or *by a routine*.
Every panel that starts an agent — the crew list, the rail, the island — was handing it an empty
head. This document previously said all three start paths went through the composer; that was
untrue, and now it is true.

The composition moved out of `server.rs` into `brief.rs`, where four tests hold it: an agent with
nothing to say is told nothing rather than handed an empty line, a plain start still carries what
the crew knows, a task keeps the first word, and the sections stay in the order that puts the work
before the housekeeping. Verified in the running app: a memory proposed and approved in the panel
appeared in the opening brief of an agent started with no task at all.

### X gets a desk

Dispatch was the last capability with an API and no face. The panel shows whether X is on duty or
holding everything, the two caps it decides by, the cards still waiting for an owner, and every
decision it has made with the reason attached.

`ask X` on a card runs the same policy the board and the MCP tools run, and the answer comes back in
the panel's own words: *assign to Ada · Ada is the free agent on agentland-svc-demo with the closest
role (implementer)*. Dropping **per repository** to 1 and asking again turned the next card amber —
*queue · 1 of 1 allowed agents are already working on agentland-svc-demo* — and the header started
counting `1 queued: t5`. The caps are not decoration; they are the policy, and now they are visible
and adjustable where the decisions are read.

Adding it found a smaller thing: `lib/core.ts` already carried a thinner dispatch client from an
earlier milestone — no caps call, a loosely typed decision, an unencoded id in the path. Two
declarations of the same thing is one too many, so the older one is gone.

### Approvals, with the answer attached

The last one. Approvals were visible per agent in the island's sheet and on the phone, but there was
nowhere to see everyone who is blocked at once — and nothing in the interface used the field the
endpoint has always accepted: a note back.

The panel lists who is waiting with the detail they attached, offers approve and reject with a note,
and keeps the answered ones with what you said. A blocked agent's terminal is one click away, since
the next question is usually *what is it actually doing*.

The note is not decoration either: `approval_status`, the MCP tool an agent polls, returns the whole
approval including `answered_note`, so *yes, but push the branch first* reaches the agent that
asked. Verified by answering from the panel and reading it back the way an agent would.

Two files this time — the panel and one registry entry. The client calls already existed.

### X stops forgetting

Every store moved to SQLite except one: dispatch lived in a mutex in `AppState`, so the caps, the
queue and the record of what X handed to whom vanished with the window. Nobody noticed while there
was no interface; the panel made it obvious, because a reason you can read until you close the app
is not a record.

Dispatch is now a store like the others — `snapshot`, `set_paused`, `set_caps`, `decide`,
`record_assignment`, `enqueue` — each write persisted, and the handlers no longer reach for a lock.
Four tests cover the reopening: the history keeps its sequence rather than restarting at one, the
caps and the pause come back, a card queued twice is queued once, and assigning a queued card takes
it out for good.

Verified the long way round: X assigned `t12` to zen with its reason, the app was closed, the row
was read straight out of `agentland.db` with nothing running, and the same decision came back
through the API after a restart.

## Recall, hybrid

Section 06 F asked for lexical scan plus embeddings, "since code memory is mostly exact-match", and
what existed was neither: `approved_for` returned every approved memory for a scope and the brief
carried all of them.

**Where the vectors come from.** The core's `reqwest` is built without TLS, so it can only reach
`http://` — which settles the design rather than limiting it. The embedder is a local endpoint: any
OpenAI-compatible `/v1/embeddings`, which is what Ollama, llama.cpp's server and LM Studio all
serve. An `https://` endpoint is refused with the reason. Nothing an agent has learned leaves the
machine.

**How the two halves are weighted.** Words first: a token match scores 1, and a token that looks
like an identifier — it carries an underscore or a digit — scores 2, because `PORT_4103` is worth
more than `the`. A vector only contributes when it clears a floor, 0.5 by default and adjustable
next to the endpoint, and then only as 35% of the score. So an exact identifier wins outright, and
the vector's job is to rescue a memory the words would have dropped.

Measured, with an embedder configured:

| Query | What came back |
|---|---|
| `PORT_4103` | one hit, score **1.00**, words 1.00, vector 0.23 — the weak vector ignored below the floor |
| *which listener does a worktree get* | one hit, score **0.46**, words 0.40, vector **0.56** — the vector lifted it |
| the same, embedder off | the same memory, on words alone |

The brief now asks recall the same question with the card's own text, capped at six. Assigning *make
the listener per worktree explicit* to an agent put exactly one memory in its opening brief —
*worktrees each get their own listener at creation* — where before it would have carried all four.

**What is verified and what is not.** There is no embedding model on this machine, so a stand-in
server was written that returns a deterministic hash of the words. That proves the wire format, both
response shapes, the storage, the floor and the ranking end to end. It proves nothing about whether
a real model ranks these memories well. The panel says `words only` until an endpoint answers, and
says how many dimensions it answered with when one does.

One thing the work tightened: the approval gate now covers the embedder. An unapproved memory is not
sent anywhere, not even to a model on localhost.

## The commander, part one

The AppImage was opened again — this time without running it: the ELF header gives the offset of the
embedded filesystem, and `unsquashfs` reads it in place. What that showed is that the other tool's
leader is not a service. It is **a real CLI agent in a pane**, with a supervisor in the main process
that writes into that agent's terminal only when its input box is idle, journals every delegation to
disk, and detects completion from several signals at once. Their own comments say why: thirty fixes
failed because the follow-up logic lived in the renderer, and a reload vaporised it.

Ours was a forty-line dispatcher: role affinity, caps, a reason. This is the first of three parts.

**A plan is a first-class thing.** A goal, a repository, and steps that name what they need — by
another step's title or its id. Steps with no dependency start at once; a step whose needs are done
becomes ready; a plan closes itself when its last step does. Two steps that wait for each other are
refused at creation with the names of the steps that are stuck, because a cycle is not a plan. Nine
tests hold the model, including that what is ready survives a restart.

**X has an identity.** Hiring an agent with the role `commander` installs a built-in skill that says
what commanding is: take the goal apart before handing anything out, a step is one agent's work,
name what waits on what, then work the plan and never report a step done because an agent said so.
The brief now opens with who the agent is and who is on the crew — the other tool's own requirement
list calls this the identity carrier, without which you cannot tell a pane "you are the leader".

**Four tools.** `plan_create`, `plan_ready`, `plan_status`, `plan_step_done`, beside the delegation
and board tools X already had.

Given a real goal — *give svc-demo a /health endpoint with a test and a readme line* — X wrote:

| Step | Waits for |
|---|---|
| Serve /health from server.js | — |
| Prove /health with a node test | Serve /health |
| Note /health in the README | — |

Two can start at once and one waits for the endpoint it tests. That is the reasoning the feature
exists for, and it was the engine's, not a fixture's.

One honest note from the same session: the first goal I gave X was already finished in the
repository, and X refused to plan it — it read the code, moved the card to review and asked whether
a plan was still wanted. That is the right answer, and it is why the run above uses a goal that
genuinely does not exist yet.

Still to build: the supervisor. Today nothing watches whether a delegated step ever landed, nothing
detects that a worker finished, and nothing wakes X when it did. That is part three, and the
other tool's design notes are a good map of the failures to skip.

## The supervisor

Thirty fixes failed before they moved the follow-up out of the
renderer, where a reload vaporised every timer, and into the main process with a journal on disk.
Ours starts where theirs ended up: in the core, in SQLite.

**What it watches.** Assigning a card that belongs to a plan step opens a watch — plan, step, card,
agent, session, and a fingerprint of the brief. Every ten seconds it looks: is the session alive,
what does the pane show, is the worktree changed, did the card get evidence.

**What it decides.** The brief is only *delivered* once the pane echoes it; if the grace period
passes without that, it types it again, twice at most, and then says the brief never reached the
agent rather than waiting forever. A step is finished when the pane prints `DONE:<step>`, or the
session exits, or the card gains evidence, or the agent is waiting at an empty prompt with a changed
worktree. Settling attaches the reason to the card and puts the news in a queue for the commander.

**How it wakes the commander.** Only when it is safe: no turn running, an empty composer now, and an
empty composer a moment ago. If the leader is busy the news waits and is delivered later as *while
you were working*, with backoff and a cap on attempts.

Three defects that only measurement could have found, each one the difference between working and
silently doing nothing:

- **Byte-idleness never settles.** A live TUI redraws its footer forever: across a real session the
  agent's idle counter cycled between 6 and 64 seconds and never once crossed ninety, so "quiet for
  90s" would have marked nothing finished, ever. The rule now reads the prompt instead — a stable
  empty composer with no turn running — and byte-idleness is only the fallback for plain shells.
- **"esc to interrupt" is not how this engine says it is busy.** In the recorded sessions that
  string appears once; the spinner line — `✶ Skedaddling… (3s · thinking)` — appears 547 times.
  Keying on the hint alone would have typed into running turns all day.
- **Two identical reads never happen.** The pane log is append-only and the footer redraws, so
  demanding a still buffer meant the leader could never be woken. What is compared now is the
  composer line, which is what the guard is actually protecting.

A fourth came from a real frame: the engine prints redraw fragments *after* the prompt, so reading
the last line concludes there is no composer at all. The composer is the last prompt in the visible
chrome, not the last line. Four fixture files recorded from real sessions hold all of this.

**The whole loop, run once end to end.** X planned `/health` into three steps; a step went to Ada;
Ada wrote the endpoint, the test and the README note; the supervisor noticed on its own — *ada
finished and left 4 changed file(s)* — and woke X, which read the evidence and closed the plan. Its
notes are what the skill asked for and not what an agent claimed:

> Verified by X on ada-tree @ e1e1872: `PORT=4190 node server.js`, curl /health → HTTP 200,
> content-type application/json … Deviation from brief: uptime is `Date.now()-started_at` floored
> rather than `process.uptime()`; equivalent for the goal.

> X ran `npm test` in the ada-tree worktree: 6 tests, 6 pass, 0 fail.

Still missing from the other tool's design: ghost-pane reaping, and delivery verification through a
transcript rather than the visible buffer.
