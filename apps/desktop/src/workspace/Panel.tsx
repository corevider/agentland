import { Component, type ErrorInfo, type ReactNode } from "react";

interface BoundaryProps {
    label: string;
    children: ReactNode;
}

interface BoundaryState {
    error: string | null;
}

export class PanelBoundary extends Component<BoundaryProps, BoundaryState> {
    state: BoundaryState = { error: null };

    static getDerivedStateFromError(error: unknown): BoundaryState {
        return { error: error instanceof Error ? error.message : String(error) };
    }

    componentDidCatch(error: Error, info: ErrorInfo) {
        console.error(`panel ${this.props.label} failed`, error, info);
    }

    render() {
        if (this.state.error) {
            return (
                <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-auto p-2.5">
                    <div className="font-mono text-[11px] text-coral">{this.props.label} stopped working</div>
                    <pre className="whitespace-pre-wrap font-mono text-[11px] text-driftwood">
                        {this.state.error}
                    </pre>
                    <button
                        className="w-fit rounded-lg border border-foam px-3 py-1 font-mono text-[11px]"
                        onClick={() => this.setState({ error: null })}
                    >
                        try again
                    </button>
                </div>
            );
        }

        return this.props.children;
    }
}

interface PanelProps {
    title: string;
    subtitle?: string;
    actions?: ReactNode;
    children: ReactNode;
}

export function Panel({ title, subtitle, actions, children }: PanelProps) {
    return (
        <section className="flex h-full min-h-0 w-full min-w-0 flex-1 flex-col overflow-hidden rounded-xl border border-reef bg-lagoon">
            <header className="flex shrink-0 items-center justify-between gap-3 border-b border-reef/70 px-2 py-1">
                <div className="flex min-w-0 items-baseline gap-2">
                    <span className="truncate text-[13px] text-linen">{title}</span>
                    {subtitle ? (
                        <span className="truncate font-mono text-[10px] text-shell">{subtitle}</span>
                    ) : null}
                </div>
                {actions ? <div className="flex shrink-0 items-center gap-1">{actions}</div> : null}
            </header>

            <PanelBoundary label={title}>
                <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">{children}</div>
            </PanelBoundary>
        </section>
    );
}
