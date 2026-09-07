/// Photographing the window, without paying for it twice.
///
/// A picture of the interface is made by serialising the DOM into an SVG, which
/// means every web font this app uses is fetched, base64'd and written into that
/// string on every capture — several megabytes of it — and then rasterised into
/// a canvas the size of the window. Measured on this machine: one capture of a
/// 1853×1001 window cost about 220 MB while it ran and left roughly 8 MB behind,
/// and eight of them took the webview from 364 MB to 426 MB and kept it there.
/// A renderer at that size on a machine with a gigabyte free is a renderer that
/// dies with SIGSEGV, which is what took the window down.
///
/// So: the fonts are embedded once and reused, the canvas gives its backing
/// store up as soon as the picture is out of it, and two captures never run at
/// the same time.
let embedded_fonts: Promise<string> | null = null;
let taking: Promise<string> | null = null;

async function fonts(node: HTMLElement): Promise<string> {
    if (!embedded_fonts) {
        const { getFontEmbedCSS } = await import("html-to-image");
        embedded_fonts = getFontEmbedCSS(node).catch(() => "");
    }

    return embedded_fonts;
}

async function draw(node: HTMLElement, background?: string): Promise<string> {
    const { toCanvas } = await import("html-to-image");
    const canvas = await toCanvas(node, {
        pixelRatio: 1,
        fontEmbedCSS: await fonts(node),
        ...(background ? { backgroundColor: background } : {}),
    });

    try {
        return canvas.toDataURL("image/png");
    } finally {
        // A canvas holds its pixels until something says otherwise, and nothing
        // was saying otherwise: eight window-sized backing stores is 60 MB the
        // webview never gave back.
        canvas.width = 0;
        canvas.height = 0;
    }
}

/// A picture of a piece of the window, as a PNG data URL.
///
/// Asked for again while one is being taken, the same picture is handed to both
/// callers rather than starting a second — two at once is two window-sized
/// canvases and two copies of every font, at the moment memory is already tight.
export function photograph(node: HTMLElement, background?: string): Promise<string> {
    if (taking) {
        return taking;
    }

    taking = draw(node, background).finally(() => {
        taking = null;
    });

    return taking;
}

/// Forget the embedded fonts, so the next capture reads them again. For a test,
/// and for the rare case of a font arriving after the first picture was taken.
export function forget_the_fonts(): void {
    embedded_fonts = null;
}
