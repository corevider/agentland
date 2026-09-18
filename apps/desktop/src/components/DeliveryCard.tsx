import { useEffect, useState } from "react";
import { delivery_preview, delivery_run, delivery_stage, type Task } from "@/lib/core";

export function DeliveryCard({task, on_changed}: {task: Task; on_changed: () => void}) {
    const [message, set_message] = useState("");
    const [status, set_status] = useState("");
    const [busy, set_busy] = useState(false);
    const [enabled, set_enabled] = useState("");
    useEffect(() => {
        let live = true;
        delivery_preview(task.id).then((preview) => { if (live) { set_message(preview.message); set_enabled(Object.entries(preview.policy).filter(([key, value]) => key.startsWith("auto_") && value === true).map(([key]) => key.slice(5)).join(", ") || "none"); } }).catch((cause) => { if (live) set_status(String(cause)); });
        return () => { live = false; };
    }, [task.id, task.title]);
    if (!task.worktree) return null;
    const run = async (stage?: "commit" | "push" | "pr") => {
        set_busy(true); set_status("");
        try {
            if (stage) { const result = await delivery_stage(task, stage, message) as {created?: boolean; detail?: string; url?: string}; set_status(stage === "pr" && result.created === false ? `${result.detail ?? "Finish opening the PR"} ${result.url ?? ""}` : `${stage} completed`); }
            else { const result = await delivery_run(task.id); set_status(`Completed: ${result.performed.join(", ") || "no steps"}${result.waiting ? `. Waiting: ${result.waiting}` : ""}`); }
            on_changed();
        } catch (cause) { set_status(String(cause)); }
        finally { set_busy(false); }
    };
    return <details className="rounded-lg border border-reef p-2 font-mono text-[11px]">
        <summary>Git workflow · selected: {enabled}</summary>
        <p className="my-2 text-shade">Run follows project settings. Individual buttons explicitly run only that step. Merge remains under the card’s review checks.</p>
        <input aria-label="Commit message" className="mb-2 w-full rounded border border-reef bg-lagoon p-1" value={message} onChange={(event) => set_message(event.target.value)} />
        <div className="flex flex-wrap gap-2">
            <button disabled={busy} onClick={() => void run()} className="rounded border border-reef px-2 py-1">run selected steps</button>
            {(["commit", "push", "pr"] as const).map((stage) => <button key={stage} disabled={busy || (stage !== "push" && !message.trim())} onClick={() => void run(stage)} className="rounded border border-reef px-2 py-1">{stage === "pr" ? "open PR" : stage}</button>)}
        </div>
        {status && <p role="status" className="mt-2 whitespace-pre-wrap">{status}</p>}
    </details>;
}
