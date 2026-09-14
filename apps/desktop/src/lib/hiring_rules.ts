import { list_engines, read_hiring, type Engine, type HireableLogin, type HiringReport } from "@/lib/core";

/// How a login is named in the rules: the engine alone for the login this
/// machine is signed in as, `engine/label` for one added beside it.
export function login_key(engine_id: string, account: string | null): string {
    return account ? `${engine_id}/${account}` : engine_id;
}

/// What is known about a login's allowance, in a few words.
export function login_words(login: HireableLogin): string {
    const said: string[] = [];

    if (login.signed_in === false) {
        said.push("nobody signed in");
    }
    if (login.weekly_percent === null && login.session_percent === null) {
        said.push("no reading yet");
    } else {
        if (login.weekly_percent !== null) {
            said.push(`week ${Math.round(login.weekly_percent)}%`);
        }
        if (login.session_percent !== null) {
            said.push(`5h ${Math.round(login.session_percent)}%`);
        }
    }
    if (login.room !== "plenty") {
        said.push(login.room);
    }

    return said.join(" · ");
}

/// The engines a picker offers: installed, and not closed to the crew.
export function open_to_the_crew(engines: Engine[], report: HiringReport | null): Engine[] {
    const closed = new Set(report?.rules.closed_engines ?? []);
    return engines.filter((engine) => engine.installed && !closed.has(engine.id));
}

/// The engines a picker offers, asked of the core.
export async function hireable_engines(): Promise<Engine[]> {
    const [engines, report] = await Promise.all([list_engines(), read_hiring().catch(() => null)]);
    return open_to_the_crew(engines, report);
}
