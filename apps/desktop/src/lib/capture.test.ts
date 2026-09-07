import { beforeEach, describe, expect, it, vi } from "vitest";

const font_calls = vi.fn();
const canvas_calls = vi.fn();
const drawn: { width: number; height: number }[] = [];

vi.mock("html-to-image", () => ({
    getFontEmbedCSS: async () => {
        font_calls();
        return "@font-face{src:url(data:font/woff2;base64,AAAA)}";
    },
    toCanvas: async (_node: unknown, options: { fontEmbedCSS?: string }) => {
        canvas_calls(options.fontEmbedCSS);
        await new Promise((done) => setTimeout(done, 5));

        const canvas = {
            width: 1853,
            height: 1001,
            toDataURL: () => "data:image/png;base64,AAA",
        };
        drawn.push(canvas);
        return canvas as unknown as HTMLCanvasElement;
    },
}));

const { photograph, forget_the_fonts } = await import("@/lib/capture");

// The helper only passes the node through, so a stand-in is enough: these tests
// run without a DOM, which is the point — what is being checked is what the
// capture holds on to, not what it draws.
const node = {} as HTMLElement;

describe("photographing the window", () => {
    beforeEach(() => {
        font_calls.mockClear();
        canvas_calls.mockClear();
        drawn.length = 0;
        forget_the_fonts();
    });

    it("embeds the fonts once and hands them to every picture after", async () => {
        await photograph(node);
        await photograph(node);
        await photograph(node);

        expect(font_calls).toHaveBeenCalledTimes(1);
        expect(canvas_calls).toHaveBeenCalledTimes(3);
        for (const call of canvas_calls.mock.calls) {
            expect(call[0]).toContain("@font-face");
        }
    });

    it("gives the canvas its pixels back rather than leaving a window-sized one behind", async () => {
        await photograph(node);

        expect(drawn).toHaveLength(1);
        expect(drawn[0].width).toBe(0);
        expect(drawn[0].height).toBe(0);
    });

    it("takes one picture for two callers rather than two at once", async () => {
        const [one, two] = await Promise.all([photograph(node), photograph(node)]);

        expect(canvas_calls).toHaveBeenCalledTimes(1);
        expect(one).toBe(two);
    });

    it("lets the next caller start once the one before it is done", async () => {
        await photograph(node);
        await photograph(node);

        expect(canvas_calls).toHaveBeenCalledTimes(2);
    });
});
