import { describe, expect, it } from "vitest";

import { resolve_renderer } from "@/lib/settings";

describe("choosing a terminal renderer", () => {
    it("draws with the DOM on WebKit, where a WebGL canvas shows a paint late", () => {
        expect(resolve_renderer("auto", "tauri-webkitgtk")).toBe("dom");
        expect(resolve_renderer("auto", "webkit")).toBe("dom");
    });

    it("takes WebGL everywhere else", () => {
        expect(resolve_renderer("auto", "chromium")).toBe("webgl");
        expect(resolve_renderer("auto", "tauri-webview")).toBe("webgl");
        expect(resolve_renderer("auto", "firefox")).toBe("webgl");
    });

    it("does what it is told when told", () => {
        expect(resolve_renderer("webgl", "tauri-webkitgtk")).toBe("webgl");
        expect(resolve_renderer("dom", "chromium")).toBe("dom");
    });
});
