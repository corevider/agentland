import { describe, expect, it } from "vitest";

import { resolve_renderer } from "@/lib/settings";

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
