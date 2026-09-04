import { describe, expect, it } from "vitest";

import {
    files_from_drop,
    files_from_paste,
    is_a_placeholder_name,
    is_image,
    stamped_name,
} from "@/lib/attachments";

const at = new Date(2026, 8, 4, 14, 5, 9);

function item(file: File | null, kind = "file") {
    return { kind, type: file?.type ?? "text/plain", getAsFile: () => file };
}

describe("what a paste carries", () => {
    it("names a screenshot by the moment it was pasted", () => {
        expect(stamped_name("image/png", at)).toBe("pasted-20260904-140509.png");
        expect(stamped_name("image/jpeg", at)).toBe("pasted-20260904-140509.jpg");
        expect(stamped_name("application/pdf", at)).toBe("pasted-20260904-140509.pdf");
    });

    it("knows a name the browser made up", () => {
        expect(is_a_placeholder_name("image.png")).toBe(true);
        expect(is_a_placeholder_name("blob")).toBe(true);
        expect(is_a_placeholder_name("Screen Shot 2026-08-13 at 17.20.45.png")).toBe(false);
        expect(is_a_placeholder_name("design.png")).toBe(false);
    });

    it("keeps a pasted picture and renames the nameless one", () => {
        const shot = new File(["png"], "image.png", { type: "image/png" });
        const kept = new File(["png"], "mockup.png", { type: "image/png" });

        const files = files_from_paste({ items: [item(shot), item(null, "string"), item(kept)] }, at);

        expect(files.map((file) => file.name)).toEqual(["pasted-20260904-140509.png", "mockup.png"]);
        expect(files[0].type).toBe("image/png");
    });

    it("leaves a paste of plain text to the browser", () => {
        expect(files_from_paste({ items: [item(null, "string")] }, at)).toEqual([]);
        expect(files_from_paste(null, at)).toEqual([]);
    });

    it("takes every dropped file, pictures or not", () => {
        const log = new File(["boom"], "server.log", { type: "text/plain" });
        const shot = new File(["png"], "shot.png", { type: "image/png" });

        const files = files_from_drop({ files: [log, shot] }, at);

        expect(files.map((file) => file.name)).toEqual(["server.log", "shot.png"]);
        expect(files_from_drop(null, at)).toEqual([]);
    });

    it("tells a picture from a file", () => {
        expect(is_image("image/png")).toBe(true);
        expect(is_image("application/pdf")).toBe(false);
    });
});
