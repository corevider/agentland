export interface GpuReport {
    webgl2: boolean;
    renderer: string;
    max_contexts: number;
}

function unmasked_renderer(context: WebGLRenderingContext | WebGL2RenderingContext): string {
    const info = context.getExtension("WEBGL_debug_renderer_info");
    if (info) {
        const value = context.getParameter(info.UNMASKED_RENDERER_WEBGL);
        if (typeof value === "string" && value.length > 0) {
            return value;
        }
    }

    const fallback = context.getParameter(context.RENDERER);
    return typeof fallback === "string" ? fallback : "unknown";
}

export function probe_gpu(limit = 16): GpuReport {
    const probe = document.createElement("canvas");
    const context =
        (probe.getContext("webgl2") as WebGL2RenderingContext | null) ??
        (probe.getContext("webgl") as WebGLRenderingContext | null);

    if (!context) {
        return { webgl2: false, renderer: "none", max_contexts: 0 };
    }

    const report: GpuReport = {
        webgl2: probe.getContext("webgl2") !== null,
        renderer: unmasked_renderer(context),
        max_contexts: 0,
    };

    const held: Array<WebGLRenderingContext | WebGL2RenderingContext> = [];
    for (let index = 0; index < limit; index += 1) {
        const canvas = document.createElement("canvas");
        canvas.width = 2;
        canvas.height = 2;
        const extra = canvas.getContext("webgl2") ?? canvas.getContext("webgl");
        if (!extra) {
            break;
        }
        held.push(extra as WebGLRenderingContext);
    }

    report.max_contexts = held.length;

    for (const entry of held) {
        entry.getExtension("WEBGL_lose_context")?.loseContext();
    }

    return report;
}
