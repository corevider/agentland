/// Recording in the window itself, with nothing installed on the machine.
///
/// The alternative is a recorder on PATH — pw-record, parec, arecord — which
/// Windows has none of, so voice was simply unavailable there. The window is a
/// browser and a browser has a microphone, so this takes the audio at the
/// source: sixteen-kilohertz mono, which is what every speech model wants, and
/// a wav written here rather than a webm the core would have to hand to ffmpeg.
/// Nothing is encoded, nothing is decoded, and no process is spawned for it.

const RATE = 16000;
const A_SAMPLE = 2;
const HEADER = 44;
const FULL_SCALE = 0x7fff;

export interface Listening {
    /// Stop, and hand back what was said as a wav nobody has to convert.
    stop: () => Promise<Blob>;
}

/// Sixteen-bit samples, as a wav says them: little-endian, signed, clamped.
export function pcm_from(samples: Float32Array): Int16Array {
    const out = new Int16Array(samples.length);

    for (let at = 0; at < samples.length; at += 1) {
        const held = Math.max(-1, Math.min(1, samples[at]));
        out[at] = Math.round(held * FULL_SCALE);
    }

    return out;
}

/// A wav header for mono PCM at one rate, written in front of the samples.
export function wav_from(samples: Int16Array, rate = RATE): Blob {
    const bytes = new ArrayBuffer(HEADER + samples.length * A_SAMPLE);
    const view = new DataView(bytes);

    const text = (at: number, value: string) => {
        for (let step = 0; step < value.length; step += 1) {
            view.setUint8(at + step, value.charCodeAt(step));
        }
    };

    text(0, "RIFF");
    view.setUint32(4, 36 + samples.length * A_SAMPLE, true);
    text(8, "WAVE");
    text(12, "fmt ");
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true);
    view.setUint16(22, 1, true);
    view.setUint32(24, rate, true);
    view.setUint32(28, rate * A_SAMPLE, true);
    view.setUint16(32, A_SAMPLE, true);
    view.setUint16(34, 16, true);
    text(36, "data");
    view.setUint32(40, samples.length * A_SAMPLE, true);

    for (let at = 0; at < samples.length; at += 1) {
        view.setInt16(HEADER + at * A_SAMPLE, samples[at], true);
    }

    return new Blob([bytes], { type: "audio/wav" });
}

export function joined(pieces: Float32Array[]): Float32Array {
    const total = pieces.reduce((sum, piece) => sum + piece.length, 0);
    const out = new Float32Array(total);

    let at = 0;
    for (const piece of pieces) {
        out.set(piece, at);
        at += piece.length;
    }

    return out;
}

/// Whether this window can record at all, before anything is held down.
export function can_listen(): boolean {
    if (typeof navigator === "undefined" || typeof window === "undefined") {
        return false;
    }

    const has_audio = Boolean(
        window.AudioContext ?? (window as unknown as { webkitAudioContext?: unknown }).webkitAudioContext,
    );

    return Boolean(navigator.mediaDevices?.getUserMedia) && has_audio;
}

/// Open the microphone and keep every sample until told to stop.
///
/// The context is asked for sixteen kilohertz outright, so the resampling is
/// the browser's own and the samples arrive at the rate they are wanted at.
/// A context that refuses the rate still works — what it did give is recorded
/// in the header, and the transcriber reads it from there.
export async function listen(): Promise<Listening> {
    const stream = await navigator.mediaDevices.getUserMedia({
        audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
    });

    const context = new AudioContext({ sampleRate: RATE });
    const source = context.createMediaStreamSource(stream);
    const pieces: Float32Array[] = [];

    // A worklet is the modern way and needs a second file to be served; this
    // is one node, deprecated but present in every engine the app runs in.
    const tap = context.createScriptProcessor(4096, 1, 1);
    tap.onaudioprocess = (event) => {
        pieces.push(new Float32Array(event.inputBuffer.getChannelData(0)));
    };

    source.connect(tap);
    // Chromium runs a ScriptProcessor only while it reaches the destination,
    // and a gain of zero keeps the microphone out of the speakers.
    const quiet = context.createGain();
    quiet.gain.value = 0;
    tap.connect(quiet);
    quiet.connect(context.destination);

    return {
        stop: async () => {
            tap.disconnect();
            quiet.disconnect();
            source.disconnect();
            stream.getTracks().forEach((track) => track.stop());

            const rate = context.sampleRate;
            await context.close();

            return wav_from(pcm_from(joined(pieces)), rate);
        },
    };
}
