import { useState } from "react";

import { UpdatesSection } from "@/components/UpdatesSection";
import { PhoneSection } from "@/components/PhoneSection";
import { StandardsSection } from "@/components/StandardsSection";
import { VoiceSection } from "@/components/VoiceSection";
import type { GpuReport } from "@/lib/gpu";
import type { Settings } from "@/lib/settings";

const PANE_CHOICES = [1, 2, 4, 8, 12];
const RATE_CHOICES = [1_000, 5_000, 10_000, 20_000, 50_000];
const DURATION_CHOICES = [10_000, 30_000, 60_000];

type SectionId = "updates" | "phone" | "standards" | "voice" | "benchmark" | "terminal" | "diagnostics";

const SECTIONS: Array<{ id: SectionId; label: string; hint: string }> = [
    { id: "updates", label: "Updates", hint: "What version this is, and what is out" },
    { id: "phone", label: "Phone", hint: "Point a camera at it and you are in" },
    { id: "standards", label: "House rules", hint: "What every agent is told, every turn" },
    { id: "voice", label: "Voice", hint: "Speaking to the crew instead of typing" },
    { id: "benchmark", label: "Benchmark", hint: "Load used by the throughput gate" },
    { id: "terminal", label: "Terminal", hint: "How panes render under load" },
    { id: "diagnostics", label: "Diagnostics", hint: "What this machine reports" },
];

interface Props {
    settings: Settings;
    on_change: (next: Settings) => void;
    on_close: () => void;
    gpu: GpuReport;
    surface: string;
    busy: boolean;
    on_run_benchmark: () => void;
    on_open_shells: () => void;
    on_clear: () => void;
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
    return (
        <div className="flex flex-wrap items-center justify-between gap-4 border-b border-reef/60 py-3">
            <div>
                <div className="text-sm text-linen">{label}</div>
                {hint ? <div className="font-mono text-[11px] text-shell">{hint}</div> : null}
            </div>
            {children}
        </div>
    );
}

function Select({
    value,
    options,
    format,
    on_change,
}: {
    value: number;
    options: number[];
    format?: (value: number) => string;
    on_change: (value: number) => void;
}) {
    return (
        <select
            className="border border-reef bg-lagoon-deep px-2 py-1 font-mono text-xs rounded-lg"
            value={value}
            onChange={(event) => on_change(Number(event.target.value))}
        >
            {options.map((option) => (
                <option key={option} value={option}>
                    {format ? format(option) : option}
                </option>
            ))}
        </select>
    );
}

export function SettingsPage({
    settings,
    on_change,
    on_close,
    gpu,
    surface,
    busy,
    on_run_benchmark,
    on_open_shells,
    on_clear,
}: Props) {
    const [section, set_section] = useState<SectionId>("updates");

    return (
        <div className="absolute inset-0 z-10 flex flex-col bg-lagoon-deep">
            <header className="flex items-center justify-between border-b border-reef px-4 py-3">
                <span className="font-display text-[17px] font-semibold text-linen">Settings</span>
                <button
                    className="border border-foam px-3 py-1 font-mono text-xs rounded-lg"
                    onClick={on_close}
                    autoFocus
                >
                    close
                </button>
            </header>

            <div className="flex min-h-0 flex-1">
                <nav className="w-56 shrink-0 border-r border-reef p-2">
                    {SECTIONS.map((entry) => (
                        <button
                            key={entry.id}
                            className={`mb-1 block w-full px-3 py-2 text-left ${
                                section === entry.id
                                    ? "bg-shallow text-turquoise"
                                    : "text-driftwood hover:bg-lagoon"
                            }`}
                            onClick={() => set_section(entry.id)}
                        >
                            <div className="text-sm">{entry.label}</div>
                            <div className="font-mono text-[10px] text-shell">{entry.hint}</div>
                        </button>
                    ))}
                </nav>

                <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
                    {section === "updates" ? <UpdatesSection /> : null}

                    {section === "phone" ? <PhoneSection /> : null}

                    {section === "standards" ? <StandardsSection /> : null}

                    {section === "voice" ? <VoiceSection /> : null}

                    {section === "benchmark" ? (
                        <div className="max-w-2xl">
                            <p className="mb-4 max-w-prose text-sm text-driftwood">
                                The gate is eight panes at 10,000 lines per second each. Raise the load to find
                                where this machine actually breaks.
                            </p>
                            <Row label="Panes" hint="terminals opened per run">
                                <Select
                                    value={settings.panes}
                                    options={PANE_CHOICES}
                                    on_change={(panes) => on_change({ ...settings, panes })}
                                />
                            </Row>
                            <Row label="Lines per second" hint="per pane, not total">
                                <Select
                                    value={settings.lines_per_second}
                                    options={RATE_CHOICES}
                                    format={(value) => value.toLocaleString()}
                                    on_change={(lines_per_second) => on_change({ ...settings, lines_per_second })}
                                />
                            </Row>
                            <Row label="Duration" hint="how long the generators run">
                                <Select
                                    value={settings.duration_ms}
                                    options={DURATION_CHOICES}
                                    format={(value) => `${value / 1000}s`}
                                    on_change={(duration_ms) => on_change({ ...settings, duration_ms })}
                                />
                            </Row>
                            <Row
                                label="Run"
                                hint={`${settings.panes} × ${settings.lines_per_second.toLocaleString()} lps — closes every pane first, then reads the HUD`}
                            >
                                <div className="flex items-center gap-2">
                                    <button
                                        className="rounded-lg border border-turquoise/70 px-3 py-1 font-mono text-xs text-turquoise disabled:opacity-40"
                                        onClick={on_run_benchmark}
                                        disabled={busy}
                                    >
                                        run benchmark
                                    </button>
                                    <button
                                        className="rounded-lg border border-reef px-3 py-1 font-mono text-xs text-shell hover:border-foam disabled:opacity-40"
                                        onClick={on_open_shells}
                                        disabled={busy}
                                    >
                                        open shells
                                    </button>
                                </div>
                            </Row>
                            <Row label="Clear" hint="close every pane, generators and shells alike">
                                <button
                                    className="rounded-lg border border-reef px-3 py-1 font-mono text-xs text-shell hover:border-coral hover:text-coral disabled:opacity-40"
                                    onClick={on_clear}
                                    disabled={busy}
                                >
                                    clear the panes
                                </button>
                            </Row>
                        </div>
                    ) : null}

                    {section === "terminal" ? (
                        <div className="max-w-2xl">
                            <p className="mb-4 max-w-prose text-sm text-driftwood">
                                Under heavy output a pane renders what a human can read and records the rest.
                                These limits are why the gate passes.
                            </p>
                            <Row label="Focused pane" hint="writes once per animation frame">
                                <span className="font-mono text-xs text-shell">live</span>
                            </Row>
                            <Row label="Background panes" hint="throttled while another pane has focus">
                                <span className="font-mono text-xs text-shell">250 ms</span>
                            </Row>
                            <Row label="Overload tail" hint="a flooded pane keeps only its last bytes">
                                <span className="font-mono text-xs text-shell">48 KB</span>
                            </Row>
                            <Row label="Skipped output" hint="never lost — the core writes every byte to disk">
                                <span className="font-mono text-xs text-palm">sessions/&lt;id&gt;.log</span>
                            </Row>
                        </div>
                    ) : null}

                    {section === "diagnostics" ? (
                        <div className="max-w-2xl">
                            <Row label="Surface" hint="which webview is rendering this window">
                                <span className="font-mono text-xs text-shell">{surface}</span>
                            </Row>
                            <Row label="WebGL" hint="the island needs one context">
                                <span className="font-mono text-xs text-shell">
                                    {gpu.webgl2 ? "webgl2" : gpu.renderer === "none" ? "unavailable" : "webgl1"}
                                </span>
                            </Row>
                            <Row label="GPU" hint="unmasked renderer string">
                                <span className="max-w-md truncate font-mono text-xs text-shell">
                                    {gpu.renderer}
                                </span>
                            </Row>
                            <Row label="Contexts granted" hint="probed at startup">
                                <span className="font-mono text-xs text-shell">{gpu.max_contexts}</span>
                            </Row>
                        </div>
                    ) : null}
                </div>
            </div>
        </div>
    );
}
