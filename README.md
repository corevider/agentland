# Agentland

An open-source desktop workspace where a named crew of CLI coding agents works in parallel across
real git worktrees — each agent with its own branch, its own running dev server, and a preview beside
the diff.


Status: **M0 — the throughput gate.** Nothing else gets built until the numbers below are green.

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
*without* GPU acceleration. That leaves one open question, and it belongs to the island rather than
the terminals: react-three-fiber needs a WebGL context. The app now probes for one at startup and
reports it in the HUD and in every sample.

## Layout

```
crates/core/            Rust core: pty runtime, framing, local API — no UI dependency
  src/pty.rs            pty spawn, output coalescing, replay buffer, broadcast
  src/bench.rs          synthetic load generator for the gate
  src/server.rs         axum HTTP + WebSocket on 127.0.0.1, token and Host guard
  src/bin/              standalone core, so the benchmark runs without Tauri
apps/desktop/
  src-tauri/            Tauri v2 shell; starts the core in-process
  src/                  Vite + React UI, xterm panes, benchmark HUD
```

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
