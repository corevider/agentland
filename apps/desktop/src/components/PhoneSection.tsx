import { useCallback, useEffect, useState } from "react";

import { Waiting } from "@/components/Spinner";
import { phone_way_in, type PhoneWayIn } from "@/lib/core";

/// Getting a phone in.
///
/// The address carries a token, and nobody wants to type one off a screen. The
/// phone points its camera at the code and it is in — on the same network, with
/// nothing leaving it.
export function PhoneSection() {
    const [way, set_way] = useState<PhoneWayIn | null>(null);
    const [notice, set_notice] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        set_way(await phone_way_in());
    }, []);

    useEffect(() => {
        refresh().catch((cause) => set_notice(String(cause)));
    }, [refresh]);

    if (!way) {
        return <Waiting says="working out where a phone should look…" className="font-mono text-[11px] text-shade" />;
    }

    return (
        <section className="flex flex-col gap-3">
            {!way.reachable ? (
                <p className="font-mono text-[11px] text-sun">
                    The core answers only this machine, so a code for it would go nowhere. Start it
                    with <span className="text-linen">AGENTLAND_HOST=0.0.0.0</span> to let a phone on
                    your network reach it.
                </p>
            ) : null}

            {way.code ? (
                <div
                    className="w-[240px] rounded-lg bg-white p-2"
                    // The code is drawn by the core, so nothing here has to know
                    // how to make one, and the token never passes through a
                    // third party to become an image.
                    dangerouslySetInnerHTML={{ __html: way.code }}
                />
            ) : null}

            {way.urls.length > 0 ? (
                <div className="flex flex-col gap-1">
                    <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-shade">
                        or type it
                    </span>
                    {way.urls.map((url) => (
                        <code key={url} className="select-all break-all font-mono text-[11px] text-turquoise">
                            {url}
                        </code>
                    ))}
                </div>
            ) : null}

            <p className="font-mono text-[10px] text-shade">
                The page shows what is waiting for you, the crew and the board, and has a box to
                speak into — the phone's own microphone, since this machine may have none. Everything
                stays on your network.
            </p>

            {notice ? <p className="font-mono text-[11px] text-coral">{notice}</p> : null}
        </section>
    );
}
