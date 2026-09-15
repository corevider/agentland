import { describe, expect, it } from "vitest";

import { pane_renderer, resolve_renderer } from "@/lib/settings";

describe("choosing what one pane draws with", () => {
    it("opens a chief's or a commander's pane on the DOM on every surface", () => {
        expect(pane_renderer("auto", "auto", "chromium", true)).toBe("dom");
        expect(pane_renderer("auto", "auto", "tauri-webkitgtk", true)).toBe("dom");
        expect(pane_renderer("auto", "auto", "firefox", true)).toBe("dom");
    });

    it("keeps a crowned pane on the DOM even when the settings ask every pane for WebGL", () => {
        expect(pane_renderer("auto", "webgl", "chromium", true)).toBe("dom");
    });

    it("lets the pane's own footer move a crowned pane to WebGL", () => {
        expect(pane_renderer("webgl", "auto", "chromium", true)).toBe("webgl");
    });

    it("leaves every other pane to the settings", () => {
        expect(pane_renderer("auto", "auto", "chromium", false)).toBe("webgl");
        expect(pane_renderer("auto", "dom", "chromium", false)).toBe("dom");
        expect(pane_renderer("dom", "webgl", "chromium", false)).toBe("dom");
    });
});

describe("choosing a terminal renderer", () => {
    it("draws the pane somebody types into with the DOM on WebKit, where WebGL shows a paint late", () => {
        expect(resolve_renderer("auto", "tauri-webkitgtk", true)).toBe("dom");
        expect(resolve_renderer("auto", "webkit", true)).toBe("dom");
    });

    it("keeps WebGL for the agents' panes on WebKit, which stream rather than take typing", () => {
        expect(resolve_renderer("auto", "tauri-webkitgtk", false)).toBe("webgl");
    });

    it("takes WebGL everywhere else, typed into or not", () => {
        expect(resolve_renderer("auto", "chromium", true)).toBe("webgl");
        expect(resolve_renderer("auto", "tauri-webview", false)).toBe("webgl");
        expect(resolve_renderer("auto", "firefox", true)).toBe("webgl");
    });

    it("does what it is told when told", () => {
        expect(resolve_renderer("webgl", "tauri-webkitgtk", true)).toBe("webgl");
        expect(resolve_renderer("dom", "chromium", false)).toBe("dom");
    });
});
