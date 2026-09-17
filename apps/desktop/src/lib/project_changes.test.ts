import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

beforeEach(() => {
    vi.resetModules();
    vi.stubGlobal("window", Object.assign(new EventTarget(), { location: { search: "" } }));
    vi.stubGlobal("BroadcastChannel", undefined);
});
afterEach(() => vi.unstubAllGlobals());

describe("project changes reach the other panels", () => {
    it("refreshes project and active workspace data after opening a folder without a reload", async () => {
        const events = await import("@/lib/project_changes");
        const core = await import("@/lib/core");
        let registered = false;
        vi.stubGlobal("fetch", vi.fn(async (url: string, init?: RequestInit) => {
            const path = new URL(url).pathname;
            if (path === "/repos" && init?.method === "POST") {
                registered = true;
                return Response.json({ id: "new-project" });
            }
            if (path === "/repos") return Response.json(registered ? [{ id: "new-project" }] : []);
            return Response.json({ active: "workspace", workspaces: [{ id: "workspace", repository_ids: registered ? ["new-project"] : [] }] });
        }));
        expect(await core.list_repos()).toEqual([]);
        let stop = () => {};
        const refreshed = new Promise<unknown>((resolve) => {
            stop = events.watch_projects_changed(() => { void Promise.all([core.list_repos(), core.list_workspaces()]).then(resolve); });
        });
        await core.add_repo("/new-project");
        expect(await refreshed).toEqual([
            [{ id: "new-project" }],
            { active: "workspace", workspaces: [{ id: "workspace", repository_ids: ["new-project"] }] },
        ]);
        stop();
    });

    it("does not advertise failed writes or ordinary reads as project changes", async () => {
        const events = await import("@/lib/project_changes");
        const core = await import("@/lib/core");
        const heard = vi.fn();
        const stop = events.watch_projects_changed(heard);
        vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(new Response("no", { status: 400 })).mockResolvedValueOnce(Response.json([])));
        await expect(core.add_repo("/missing")).rejects.toThrow("400");
        await core.list_repos();
        expect(heard).not.toHaveBeenCalled();
        stop();
    });

    it("includes clone, start, removal, membership, settings and worktree changes", async () => {
        const { changes_projects } = await import("@/lib/project_changes");
        for (const path of ["/repos", "/start", "/repos/demo", "/repos/demo/settings", "/repos/demo/worktrees", "/workspaces/active", "/workspaces/w1"]) {
            expect(changes_projects(path, "POST")).toBe(true);
            expect(changes_projects(path, "DELETE")).toBe(true);
            expect(changes_projects(path, "GET")).toBe(false);
        }
        expect(changes_projects("/sessions/a/input", "POST")).toBe(false);
    });

    it("delivers cross-window changes once without rebroadcasting them", async () => {
        const sent = vi.fn();
        let receive: (() => void) | null = null;
        vi.stubGlobal("BroadcastChannel", class {
            postMessage = sent;
            set onmessage(listener: () => void) { receive = listener; }
        });
        const events = await import("@/lib/project_changes");
        const heard = vi.fn();
        const stop = events.watch_projects_changed(heard);
        events.projects_changed();
        expect(heard).toHaveBeenCalledTimes(1);
        expect(sent).toHaveBeenCalledTimes(1);
        (receive as unknown as () => void)();
        expect(heard).toHaveBeenCalledTimes(2);
        expect(sent).toHaveBeenCalledTimes(1);
        stop();
        events.projects_changed();
        expect(heard).toHaveBeenCalledTimes(2);
    });
});
