import type { Agent, Service } from "@/lib/core";

/// What the picker in the preview hands back about one element.
export interface Pick {
    url: string;
    selector: string;
    tag: string;
    html: string;
    text: string;
    styles: Record<string, string>;
    box: { x: number; y: number; width: number; height: number };
    viewport: { width: number; height: number };
}

const UNSAID = new Set([
    "",
    "none",
    "normal",
    "auto",
    "0px",
    "static",
    "visible",
    "rgba(0, 0, 0, 0)",
    "1",
    "start",
    "stretch",
    "row",
    "content-box",
]);

/// Whether a computed style is something the page set rather than what every
/// element has anyway. An agent handed all twenty-four reads past the three
/// that matter.
export function worth_saying(value: string): boolean {
    const held = value.trim();
    return !UNSAID.has(held) && !held.startsWith("0px none");
}

/// Who a pick goes to first: whoever works in the worktree the dev server runs
/// from, then everybody else on the project.
export function recipients(service: Service, agents: Agent[]): Agent[] {
    const on_project = agents.filter((agent) => agent.repository_id === service.repository_id);
    return [
        ...on_project.filter((agent) => agent.worktree === service.worktree),
        ...on_project.filter((agent) => agent.worktree !== service.worktree),
    ];
}

/// The page as the dev server itself serves it. The picker sees the preview's
/// address, which is Agentland's and means nothing to the agent.
export function page_on(service: Service, seen: string): string {
    try {
        const at = new URL(seen);
        return `${service.url.replace(/\/+$/, "")}${at.pathname}${at.search}${at.hash}`;
    } catch {
        return service.url;
    }
}

/// What the agent reads: the person's words first, then where the element is
/// and what it is made of — enough to find it in the code without asking.
export function design_note(pick: Pick, said: string, service: Service): string {
    const styles = Object.entries(pick.styles)
        .filter(([, value]) => worth_saying(value))
        .map(([name, value]) => `  ${name}: ${value};`);
    const text = pick.text.replace(/\s+/g, " ").trim();

    return [
        said.trim(),
        "",
        `Picked in the preview of ${service.repository_id}/${service.worktree}, on ${page_on(service, pick.url)}:`,
        `- element: ${pick.selector} (${pick.tag}, ${pick.box.width}×${pick.box.height} at ${pick.box.x},${pick.box.y} in a ${pick.viewport.width}×${pick.viewport.height} view)`,
        ...(text ? [`- its text: "${text.slice(0, 200)}"`] : []),
        "- its HTML:",
        "```html",
        pick.html,
        "```",
        ...(styles.length > 0 ? ["- the styles set on it:", "```css", ...styles, "```"] : []),
        "",
        "Find where this is rendered in the code, make the change, and say what you changed.",
    ].join("\n");
}
