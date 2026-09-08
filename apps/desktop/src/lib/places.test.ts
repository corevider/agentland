import { describe, expect, it } from "vitest";

import {
    belongs_here,
    home_from,
    needs_switch,
    place_routines,
    places_from,
    score,
    search_places,
    trail,
    type World,
} from "@/lib/places";

const world: World = {
    workspaces: [
        { id: "w1", name: "Agentland", repository_ids: ["agentland"] },
        { id: "w2", name: "Errands", repository_ids: ["svc-demo"] },
    ],
    active_workspace: "w1",
    repositories: [
        {
            id: "agentland",
            name: "agentland",
            primary_path: "/home/dev/Desktop/agentland",
            default_branch: "main",
            remotes: [],
            origin: null,
        },
        {
            id: "svc-demo",
            name: "svc-demo",
            primary_path: "/home/dev/code/svc-demo",
            default_branch: "main",
            remotes: [],
            origin: null,
        },
    ],
    worktrees: [
        {
            name: "ada-tree",
            repository_id: "svc-demo",
            path: "/home/dev/Desktop/agentland/data/worktrees/svc-demo/ada-tree",
            branch: "agent/ada-tree",
            port: 4101,
            dirty_files: 0,
            ahead: 2,
            missing: false,
        },
    ],
    agents: [
        {
            id: "ada",
            name: "Ada",
            role: "implementer",
            engine_id: "claude",
            repository_id: "svc-demo",
            worktree: "ada-tree",
            workspace_id: null,
            session_id: "pane-1",
            state: "working",
            presence: "working",
            since: 0,
            reason: "",
            model: "haiku",
            title: "Ada · /health",
            colour: "#f0a",
            permissions: null,
            account: null,
        },
    ],
};

describe("the places a person can go", () => {
    it("holds every workspace, project, worktree and agent", () => {
        const places = places_from(world);

        expect(places.filter((place) => place.kind === "workspace")).toHaveLength(2);
        expect(places.filter((place) => place.kind === "project")).toHaveLength(2);
        expect(places.filter((place) => place.kind === "worktree")).toHaveLength(1);
        expect(places.filter((place) => place.kind === "agent")).toHaveLength(1);
    });

    it("says where a project's folder is, shortened to home", () => {
        const project = places_from(world, "/home/dev").find((place) => place.id === "project:svc-demo");

        expect(project?.detail).toBe("~/code/svc-demo");
    });

    it("carries the worktree's own path, which is not the project's folder", () => {
        const tree = places_from(world).find((place) => place.kind === "worktree");

        expect(tree?.detail).toContain("agent/ada-tree");
        expect(tree?.detail).toContain("/data/worktrees/svc-demo/ada-tree");
    });

    it("knows which workspace has to be active to see a place", () => {
        const places = places_from(world);
        const ada = places.find((place) => place.agent_id === "ada");

        expect(ada?.workspace_id).toBe("w2");
        expect(needs_switch(ada!, "w1")).toBe(true);
        expect(needs_switch(ada!, "w2")).toBe(false);
    });

    it("ranks a name over a mention in the detail line", () => {
        const places = places_from(world);
        const found = search_places(places, "ada");

        expect(found[0]?.agent_id).toBe("ada");
    });

    it("finds an agent by the name the crew uses, not only the title it was given", () => {
        const found = search_places(places_from(world), "Ada · /health");

        expect(found[0]?.agent_id).toBe("ada");
        expect(search_places(places_from(world), "implementer")[0]?.agent_id).toBe("ada");
    });

    it("browses widest-first when nothing is typed", () => {
        const found = search_places(places_from(world), "");

        expect(found[0].kind).toBe("workspace");
        expect(found.findIndex((place) => place.kind === "agent")).toBeGreaterThan(
            found.findIndex((place) => place.kind === "project"),
        );
    });

    it("switches nothing when every project is already on screen", () => {
        const ada = places_from(world).find((place) => place.agent_id === "ada")!;

        expect(needs_switch(ada, null)).toBe(false);
        expect(needs_switch(places_from(world)[0], null)).toBe(true);
    });

    it("takes a project and a name in one breath", () => {
        const found = search_places(places_from(world), "svc ada");

        expect(found[0]?.agent_id).toBe("ada");
        expect(search_places(places_from(world), "svc-demo ada-tree")[0]?.kind).toBe("worktree");
    });

    it("knows an agent by the short name the crew uses, not only its title", () => {
        const places = places_from(world);
        const found = search_places(places, "svc-demo ada");

        expect(found[0]?.agent_id).toBe("ada");
        expect(score(places.find((place) => place.agent_id === "ada")!, "ada")).toBe(100);
    });

    it("takes the workspace's name as a way in", () => {
        const found = search_places(places_from(world), "errands ada");

        expect(found[0]?.agent_id).toBe("ada");
    });

    it("wants every word to land somewhere", () => {
        expect(search_places(places_from(world), "ada zzz")).toHaveLength(0);
    });

    it("finds nothing for a word nobody uses", () => {
        expect(search_places(places_from(world), "zzz")).toHaveLength(0);
    });

    it("shows everything when nothing is typed", () => {
        expect(score(places_from(world)[0], "  ")).toBe(1);
        expect(search_places(places_from(world), "").length).toBeGreaterThan(3);
    });
});

describe("the trail in the header", () => {
    it("reads workspace, project, worktree and branch", () => {
        expect(trail(world, "svc-demo", "ada-tree")).toEqual([
            "Agentland",
            "svc-demo",
            "ada-tree · agent/ada-tree",
        ]);
    });

    it("falls back to the project's own folder when no worktree is open", () => {
        expect(trail(world, "svc-demo", null, "/home/dev")).toEqual([
            "Agentland",
            "svc-demo",
            "~/code/svc-demo",
        ]);
    });

    it("says everything when no workspace is active", () => {
        expect(trail({ ...world, active_workspace: null }, null, null)[0]).toBe("everything");
    });
});

describe("finding home in the paths themselves", () => {
    it("reads it off a Linux path", () => {
        expect(home_from(["/home/dev/Desktop/agentland"])).toBe("/home/dev");
    });

    it("reads it off a Mac path", () => {
        expect(home_from(["/Users/ege/code/svc-demo"])).toBe("/Users/ege");
    });

    it("shortens nothing when the paths are elsewhere", () => {
        expect(home_from(["/srv/checkouts/app", "/opt/thing"])).toBe("");
        expect(home_from([])).toBe("");
    });
});

describe("a chief in the jumper", () => {
    const chief = {
        ...world.agents[0],
        id: "x",
        name: "X",
        role: "chief",
        repository_id: "",
        worktree: "",
        workspace_id: "w2",
    };

    it("is filed under the workspace it commands, not under no project", () => {
        const place = places_from({ ...world, agents: [chief] }, "").find(
            (held) => held.kind === "agent",
        );

        expect(place?.workspace_id).toBe("w2");
        expect(place?.detail).toBe("X · chief · Errands");
        expect(place?.repository_id).toBeNull();
    });
});

describe("which workspace an agent belongs to", () => {
    const chief = { repository_id: "", workspace_id: "ws3" };
    const commander = { repository_id: "svc", workspace_id: null };

    it("places a chief by its workspace, having no project to be placed by", () => {
        expect(belongs_here(chief, "ws3", ["svc"])).toBe(true);
        expect(belongs_here(chief, "ws1", ["svc"])).toBe(false);
    });

    it("places everybody else by their project", () => {
        expect(belongs_here(commander, "ws3", ["svc"])).toBe(true);
        expect(belongs_here(commander, "ws3", ["other"])).toBe(false);
    });

    it("keeps everyone when nothing is being narrowed to", () => {
        expect(belongs_here(chief, null, null)).toBe(true);
        expect(belongs_here(commander, null, null)).toBe(true);
    });
});

describe("where a routine's agent lives", () => {
    const workspaces = [
        { id: "w1", name: "Agentland" },
        { id: "w2", name: "Errands" },
    ];

    const crew = [
        { ...world.agents[0], id: "tor", name: "Tor", repository_id: "", worktree: "", workspace_id: "w1" },
        { ...world.agents[0], id: "x", name: "X", repository_id: "svc-demo", worktree: "x-desk", workspace_id: null },
    ];

    const routines = [
        { id: "r1", agent_id: "tor" },
        { id: "r2", agent_id: "x" },
        { id: "r3", agent_id: "long-gone" },
    ];

    it("says a chief's routine by the workspace it commands", () => {
        const { here } = place_routines(routines, crew, workspaces, "w1", ["agentland"]);

        expect(here.map((held) => held.routine.id)).toEqual(["r1"]);
        expect(here[0].who).toBe("Tor");
        expect(here[0].where).toBe("Agentland");
    });

    it("says everybody else's by project and worktree", () => {
        const { here } = place_routines(routines, crew, workspaces, "w2", ["svc-demo"]);

        expect(here.map((held) => held.routine.id)).toEqual(["r2"]);
        expect(here[0].where).toBe("svc-demo/x-desk");
    });

    it("keeps the ones from elsewhere rather than hiding what cannot be turned off", () => {
        const { here, elsewhere } = place_routines(routines, crew, workspaces, "w1", ["agentland"]);

        expect(here).toHaveLength(1);
        expect(elsewhere.map((held) => held.routine.id)).toEqual(["r2", "r3"]);
    });

    it("says so when the agent a routine names is gone", () => {
        const { elsewhere } = place_routines(routines, crew, workspaces, "w1", ["agentland"]);
        const orphan = elsewhere.find((held) => held.routine.id === "r3");

        expect(orphan?.gone).toBe(true);
        expect(orphan?.who).toBe("long-gone");
        expect(orphan?.where).toBe("that agent is gone");
    });

    it("puts everything here when nothing is being narrowed to", () => {
        const { here, elsewhere } = place_routines(routines, crew, workspaces, null, null);

        expect(here.map((held) => held.routine.id)).toEqual(["r1", "r2"]);
        expect(elsewhere.map((held) => held.routine.id)).toEqual(["r3"]);
    });
});
