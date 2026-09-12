// Served by Agentland's preview in front of a dev server, and added to every
// page it passes through. It does nothing until the window around the preview
// asks it to pick: then it outlines what the pointer is over, and a click
// hands that element — where it is, what it is made of, how it is styled —
// back to the window, which passes it to an agent.
(() => {
    if (window.__agentland_picker || window.parent === window) {
        return;
    }
    window.__agentland_picker = true;

    const STYLES = [
        "display", "position", "box-sizing", "width", "height", "margin", "padding",
        "color", "background-color", "font-family", "font-size", "font-weight",
        "line-height", "text-align", "border", "border-radius", "box-shadow", "gap",
        "flex-direction", "justify-content", "align-items", "grid-template-columns",
        "opacity", "z-index",
    ];

    let picking = false;
    let hovered = null;

    const outline = document.createElement("div");
    outline.setAttribute("data-agentland-picker", "");
    Object.assign(outline.style, {
        position: "fixed", pointerEvents: "none", zIndex: "2147483647", display: "none",
        border: "2px solid #f2c14e", background: "rgba(242, 193, 78, 0.12)", borderRadius: "2px",
    });
    const label = document.createElement("div");
    Object.assign(label.style, {
        position: "absolute", left: "-2px", top: "-20px", padding: "1px 4px", whiteSpace: "nowrap",
        font: "11px ui-monospace, monospace", color: "#0d1c1f", background: "#f2c14e",
    });
    outline.appendChild(label);

    const say = (message) => window.parent.postMessage(message, "*");

    const selector_of = (node) => {
        const steps = [];
        for (let at = node; at && at.nodeType === 1 && steps.length < 5; at = at.parentElement) {
            if (at.id) {
                steps.unshift("#" + CSS.escape(at.id));
                break;
            }
            let step = at.tagName.toLowerCase();
            const classes = Array.from(at.classList).slice(0, 2);
            if (classes.length > 0) {
                step += "." + classes.map((name) => CSS.escape(name)).join(".");
            }
            const parent = at.parentElement;
            if (parent) {
                const same = Array.from(parent.children).filter((child) => child.tagName === at.tagName);
                if (same.length > 1) {
                    step += ":nth-of-type(" + (same.indexOf(at) + 1) + ")";
                }
            }
            steps.unshift(step);
            if (at === document.body) {
                break;
            }
        }
        return steps.join(" > ");
    };

    const place = (node) => {
        const box = node.getBoundingClientRect();
        Object.assign(outline.style, {
            display: "block", left: box.left + "px", top: box.top + "px",
            width: box.width + "px", height: box.height + "px",
        });
        label.textContent = selector_of(node) + "  " + Math.round(box.width) + "×" + Math.round(box.height);
        label.style.top = box.top < 22 ? box.height + 2 + "px" : "-20px";
    };

    const start = () => {
        picking = true;
        document.documentElement.appendChild(outline);
        document.documentElement.style.cursor = "crosshair";
    };

    const stop = () => {
        picking = false;
        hovered = null;
        outline.style.display = "none";
        outline.remove();
        document.documentElement.style.cursor = "";
    };

    addEventListener("mousemove", (event) => {
        const node = event.target;
        if (!picking || !(node instanceof Element) || outline.contains(node)) {
            return;
        }
        hovered = node;
        place(node);
    }, true);

    // A click while picking picks; it does not also follow a link or press a
    // button in the page underneath.
    addEventListener("click", (event) => {
        if (!picking) {
            return;
        }
        event.preventDefault();
        event.stopPropagation();
        event.stopImmediatePropagation();

        const node = hovered || (event.target instanceof Element ? event.target : null);
        if (!node) {
            return;
        }
        const seen = getComputedStyle(node);
        const styles = {};
        for (const name of STYLES) {
            styles[name] = seen.getPropertyValue(name);
        }
        const box = node.getBoundingClientRect();
        stop();

        say({
            agentland: "picked",
            pick: {
                url: location.href,
                selector: selector_of(node),
                tag: node.tagName.toLowerCase(),
                html: node.outerHTML.slice(0, 4000),
                text: (node.innerText || "").trim().slice(0, 400),
                styles,
                box: { x: Math.round(box.x), y: Math.round(box.y), width: Math.round(box.width), height: Math.round(box.height) },
                viewport: { width: innerWidth, height: innerHeight },
            },
        });
    }, true);

    for (const kind of ["mousedown", "mouseup", "pointerdown", "pointerup"]) {
        addEventListener(kind, (event) => {
            if (picking) {
                event.preventDefault();
                event.stopPropagation();
            }
        }, true);
    }

    addEventListener("keydown", (event) => {
        if (picking && event.key === "Escape") {
            stop();
            say({ agentland: "stopped" });
        }
    }, true);

    addEventListener("message", (event) => {
        if (event.source !== window.parent || !event.data || event.data.agentland !== "pick") {
            return;
        }
        if (event.data.on) {
            start();
        } else {
            stop();
        }
    });

    // Every page load says so, so a reload or a link followed goes on picking.
    say({ agentland: "ready", url: location.href });
})();
