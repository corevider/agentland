import { describe, expect, it } from "vitest";

import { joined, pcm_from, wav_from } from "@/lib/listen";

function header_of(bytes: ArrayBuffer) {
    const view = new DataView(bytes);
    const text = (at: number, length: number) =>
        String.fromCharCode(...new Uint8Array(bytes, at, length));

    return {
        riff: text(0, 4),
        wave: text(8, 4),
        format: view.getUint16(20, true),
        channels: view.getUint16(22, true),
        rate: view.getUint32(24, true),
        bits: view.getUint16(34, true),
        data: text(36, 4),
        data_size: view.getUint32(40, true),
        total: view.getUint32(4, true),
    };
}

describe("pcm_from", () => {
    it("puts full scale at the top of a signed sixteen-bit sample", () => {
        expect(Array.from(pcm_from(new Float32Array([0, 1, -1])))).toEqual([0, 32767, -32767]);
    });

    it("clamps rather than wrapping, so a loud voice does not come back inverted", () => {
        expect(Array.from(pcm_from(new Float32Array([1.4, -2]))));
        expect(Array.from(pcm_from(new Float32Array([1.4, -2])))).toEqual([32767, -32767]);
    });
});

describe("wav_from", () => {
    it("writes a header a transcriber can read without being told anything", async () => {
        const wav = wav_from(pcm_from(new Float32Array([0, 0.5, -0.5])), 16000);
        const read = header_of(await wav.arrayBuffer());

        expect(wav.type).toBe("audio/wav");
        expect(read.riff).toBe("RIFF");
        expect(read.wave).toBe("WAVE");
        expect(read.format).toBe(1);
        expect(read.channels).toBe(1);
        expect(read.rate).toBe(16000);
        expect(read.bits).toBe(16);
        expect(read.data).toBe("data");
    });

    it("says how long it is, both ways round, so nothing reads past the end", async () => {
        const wav = wav_from(pcm_from(new Float32Array(10)), 16000);
        const read = header_of(await wav.arrayBuffer());

        expect(wav.size).toBe(44 + 20);
        expect(read.data_size).toBe(20);
        expect(read.total).toBe(36 + 20);
    });

    it("records the rate it was actually given, not the one it wanted", async () => {
        const read = header_of(await wav_from(new Int16Array(4), 48000).arrayBuffer());
        expect(read.rate).toBe(48000);
    });

    it("keeps a sample as the same number after the round trip", async () => {
        const wav = wav_from(pcm_from(new Float32Array([1, -1, 0])), 16000);
        const view = new DataView(await wav.arrayBuffer());

        expect(view.getInt16(44, true)).toBe(32767);
        expect(view.getInt16(46, true)).toBe(-32767);
        expect(view.getInt16(48, true)).toBe(0);
    });
});

describe("joined", () => {
    it("puts the pieces back in the order they were recorded", () => {
        const out = joined([new Float32Array([1, 2]), new Float32Array([3]), new Float32Array([])]);
        expect(Array.from(out)).toEqual([1, 2, 3]);
    });

    it("is empty when nothing was said", () => {
        expect(joined([]).length).toBe(0);
    });
});
