import { useEffect, useState } from "react";
import { list_engines, save_project_settings, type Engine, type ProjectSettings, type Repository } from "@/lib/core";
import { pick_folder } from "@/lib/pick";
import { Press } from "@/components/Press";

export const empty_project_settings = (): ProjectSettings => ({ engine_ids: [], commander_engine_id: null, reference_folders: [] });
const input = "min-w-0 rounded-lg border border-reef bg-lagoon px-2 py-1 font-mono text-[11px]";

export function ProjectOptions({ value, on_change, disabled = false }: {
    value: ProjectSettings;
    on_change: (settings: ProjectSettings) => void;
    disabled?: boolean;
}) {
    const [engines, set_engines] = useState<Engine[]>([]);
    const [error, set_error] = useState<string | null>(null);
    const [path, set_path] = useState("");
    const [note, set_note] = useState("");
    useEffect(() => { list_engines().then(set_engines).catch((cause) => set_error(String(cause))); }, []);
    const toggle = (id: string) => {
        const engine_ids = value.engine_ids.includes(id) ? value.engine_ids.filter((held) => held !== id) : [...value.engine_ids, id];
        on_change({ ...value, engine_ids, commander_engine_id: engine_ids.length && !engine_ids.includes(value.commander_engine_id ?? "") ? null : value.commander_engine_id });
    };
    return (
        <fieldset disabled={disabled} className="flex min-w-0 flex-col gap-2 font-mono text-[11px] disabled:opacity-60">
            <legend className="mb-2 text-shell">Project engines and references</legend>
            <div className="flex flex-wrap gap-3">
                {engines.map((engine) => <label key={engine.id} className="flex items-center gap-1">
                    <input type="checkbox" checked={value.engine_ids.includes(engine.id)} onChange={() => toggle(engine.id)} />
                    {engine.name}{!engine.installed && <span className="text-shade"> (not installed)</span>}
                </label>)}
            </div>
            <p className="text-[10px] text-shade">Select the engines this project may use. No selection inherits global hiring settings. Existing agents keep running.</p>
            <label className="flex flex-wrap items-center gap-2 text-shell">Default commander engine
                <select className={input} value={value.commander_engine_id ?? ""} onChange={(event) => on_change({ ...value, commander_engine_id: event.target.value || null })}>
                    <option value="">Choose automatically from allowed engines</option>
                    {engines.filter((engine) => engine.takes_the_tools && (!value.engine_ids.length || value.engine_ids.includes(engine.id))).map((engine) =>
                        <option key={engine.id} value={engine.id}>{engine.name}{!engine.installed ? " (not installed)" : ""}</option>)}
                </select>
            </label>
            <p className="mt-1 text-shell">Reference folders</p>
            <p className="text-[10px] text-shade">Background material for the crew, included in its next brief or session. These folders stay in place; adding one grants no permission to modify its files.</p>
            {value.reference_folders.map((reference, index) => <div key={index} className="flex min-w-0 flex-wrap items-center gap-2 rounded border border-reef p-2">
                <span className="min-w-0 flex-1 break-all">{reference.path}</span>
                <input aria-label={`Description for ${reference.path}`} className={`${input} flex-1`} placeholder="what should agents use this for?" value={reference.note} onChange={(event) => on_change({ ...value, reference_folders: value.reference_folders.map((held, at) => at === index ? { ...held, note: event.target.value } : held) })} />
                <button type="button" className="text-coral" onClick={() => on_change({ ...value, reference_folders: value.reference_folders.filter((_, at) => at !== index) })}>remove</button>
            </div>)}
            <div className="flex flex-wrap gap-2">
                <input aria-label="Reference folder path" className={`${input} flex-1`} placeholder="absolute folder path" value={path} onChange={(event) => set_path(event.target.value)} />
                <Press className={input} on_press={async () => {
                    try { const chosen = await pick_folder("Add a reference folder", path || undefined); if (chosen) set_path(chosen); }
                    catch (cause) { set_error(String(cause)); }
                }}>browse…</Press>
                <input aria-label="Reference folder description" className={`${input} flex-1`} placeholder="description (optional)" value={note} onChange={(event) => set_note(event.target.value)} />
                <button type="button" className={`${input} text-turquoise disabled:opacity-40`} disabled={!path.trim() || value.reference_folders.length >= 32} onClick={() => {
                    if (value.reference_folders.some((held) => held.path === path.trim())) { set_error("This reference folder is already listed."); return; }
                    on_change({ ...value, reference_folders: [...value.reference_folders, { path: path.trim(), note: note.trim() }] });
                    set_path(""); set_note(""); set_error(null);
                }}>add reference</button>
            </div>
            {error && <p role="alert" className="text-coral">{error}</p>}
        </fieldset>
    );
}

export function SavedProjectOptions({ repository }: { repository: Repository }) {
    const [editing, set_editing] = useState(false);
    const [draft, set_draft] = useState<ProjectSettings>(repository.settings ?? empty_project_settings());
    const [error, set_error] = useState<string | null>(null);
    const [busy, set_busy] = useState(false);
    return <div className="border-b border-reef p-2">
        {!editing ? <button className="font-mono text-[11px] text-turquoise" onClick={() => { set_draft(repository.settings ?? empty_project_settings()); set_error(null); set_editing(true); }}>
            project settings · {repository.settings?.engine_ids.join(", ") || "global engines"} · {repository.settings?.reference_folders.length ?? 0} reference folders
        </button> : <div className="flex flex-col gap-2">
            <ProjectOptions value={draft} on_change={set_draft} disabled={busy} />
            {error && <p role="alert" className="font-mono text-[11px] text-coral">{error}</p>}
            <div className="flex gap-2">
                <Press className={`${input} text-turquoise`} disabled={busy} on_press={async () => {
                    set_busy(true); set_error(null);
                    try { await save_project_settings(repository.id, draft); set_editing(false); }
                    catch (cause) { set_error(String(cause)); }
                    finally { set_busy(false); }
                }}>save project settings</Press>
                <button className={input} disabled={busy} onClick={() => set_editing(false)}>cancel</button>
            </div>
        </div>}
    </div>;
}
