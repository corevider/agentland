import { useState } from "react";

import type { GpuReport } from "@/lib/gpu";
import type { Settings } from "@/lib/settings";

const PANE_CHOICES = [1, 2, 4, 8, 12];
const RATE_CHOICES = [1_000, 5_000, 10_000, 20_000, 50_000];
const DURATION_CHOICES = [10_000, 30_000, 60_000];

type SectionId = "benchmark" | "terminal" | "diagnostics";

const SECTIONS: Array<{ id: SectionId; label: string; hint: string }> = [
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
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
    return (
        <div className="flex flex-wrap items-center justify-between gap-4 border-b border-[#1c262a] py-3">
            <div>
                <div className="text-sm text-[#e3ebee]">{label}</div>
                {hint ? <div className="font-mono text-[11px] text-[#7b8d94]">{hint}</div> : null}
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
            className="border border-[#26343a] bg-[#0d1315] px-2 py-1 font-mono text-xs"
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

export function SettingsPage({ settings, on_change, on_close, gpu, surface }: Props) {
    const [section, set_section] = useState<SectionId>("benchmark");

    return (
        <div className="absolute inset-0 z-10 flex flex-col bg-[#0b1113]">
            <header className="flex items-center justify-between border-b border-[#26343a] px-4 py-3">
                <span className="font-mono text-xs uppercase tracking-[0.14em] text-[#45bcc4]">Settings</span>
                <button
                    className="border border-[#3a4d55] px-3 py-1 font-mono text-xs"
                    onClick={on_close}
                    autoFocus
                >
                    close
                </button>
            </header>

            <div className="flex min-h-0 flex-1">
                <nav className="w-56 shrink-0 border-r border-[#26343a] p-2">
                    {SECTIONS.map((entry) => (
                        <button
                            key={entry.id}
                            className={`mb-1 block w-full px-3 py-2 text-left ${
                                section === entry.id
                                    ? "bg-[#14343a] text-[#45bcc4]"
                                    : "text-[#a4b5bb] hover:bg-[#141c1f]"
                            }`}
                            onClick={() => set_section(entry.id)}
                        >
                            <div className="text-sm">{entry.label}</div>
                            <div className="font-mono text-[10px] text-[#7b8d94]">{entry.hint}</div>
                        </button>
                    ))}
                </nav>

                <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
                    {section === "benchmark" ? (
                        <div className="max-w-2xl">
                            <p className="mb-4 max-w-prose text-sm text-[#a4b5bb]">
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
                        </div>
                    ) : null}

                    {section === "terminal" ? (
                        <div className="max-w-2xl">
                            <p className="mb-4 max-w-prose text-sm text-[#a4b5bb]">
                                Under heavy output a pane renders what a human can read and records the rest.
                                These limits are why the gate passes.
                            </p>
                            <Row label="Focused pane" hint="writes once per animation frame">
                                <span className="font-mono text-xs text-[#7b8d94]">live</span>
                            </Row>
                            <Row label="Background panes" hint="throttled while another pane has focus">
                                <span className="font-mono text-xs text-[#7b8d94]">250 ms</span>
                            </Row>
                            <Row label="Overload tail" hint="a flooded pane keeps only its last bytes">
                                <span className="font-mono text-xs text-[#7b8d94]">48 KB</span>
                            </Row>
                            <Row label="Skipped output" hint="never lost — the core writes every byte to disk">
                                <span className="font-mono text-xs text-[#5aa87c]">sessions/&lt;id&gt;.log</span>
                            </Row>
                        </div>
                    ) : null}

                    {section === "diagnostics" ? (
                        <div className="max-w-2xl">
                            <Row label="Surface" hint="which webview is rendering this window">
                                <span className="font-mono text-xs text-[#7b8d94]">{surface}</span>
                            </Row>
                            <Row label="WebGL" hint="the island needs one context">
                                <span className="font-mono text-xs text-[#7b8d94]">
                                    {gpu.webgl2 ? "webgl2" : gpu.renderer === "none" ? "unavailable" : "webgl1"}
                                </span>
                            </Row>
                            <Row label="GPU" hint="unmasked renderer string">
                                <span className="max-w-md truncate font-mono text-xs text-[#7b8d94]">
                                    {gpu.renderer}
                                </span>
                            </Row>
                            <Row label="Contexts granted" hint="probed at startup">
                                <span className="font-mono text-xs text-[#7b8d94]">{gpu.max_contexts}</span>
                            </Row>
                        </div>
                    ) : null}
                </div>
            </div>
        </div>
    );
}
