/// How a missing tool gets onto this machine, in one command.
///
/// The command is put into a fresh pane rather than run: the person reads it,
/// presses Enter, and watches it go — an installer nobody saw run is one nobody
/// trusts. Each is the tool's own recommended route for the platform.
export interface Recipe {
    tool: string;
    what: string;
    command: string;
    url: string;
}

const NOTES: Record<string, { what: string; url: string }> = {
    npm: { what: "Node.js brings npm with it", url: "https://nodejs.org" },
    cargo: { what: "rustup installs the Rust toolchain, cargo included", url: "https://rustup.rs" },
    uv: { what: "uv is Astral's Python package and project manager", url: "https://docs.astral.sh/uv/" },
    go: { what: "the Go toolchain", url: "https://go.dev/dl/" },
    git: { what: "git itself", url: "https://git-scm.com/downloads" },
};

const COMMANDS: Record<string, Record<string, string>> = {
    windows: {
        npm: "winget install OpenJS.NodeJS.LTS",
        cargo: "winget install Rustlang.Rustup",
        uv: "winget install astral-sh.uv",
        go: "winget install GoLang.Go",
        git: "winget install Git.Git",
    },
    macos: {
        npm: "brew install node",
        cargo: "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh",
        uv: "curl -LsSf https://astral.sh/uv/install.sh | sh",
        go: "brew install go",
        git: "brew install git",
    },
    linux: {
        npm: "sudo apt install -y nodejs npm",
        cargo: "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh",
        uv: "curl -LsSf https://astral.sh/uv/install.sh | sh",
        go: "sudo apt install -y golang-go",
        git: "sudo apt install -y git",
    },
};

export function recipe_for(tool: string, os: string): Recipe | null {
    const note = NOTES[tool];
    const command = COMMANDS[os]?.[tool];
    if (!note || !command) {
        return null;
    }

    return { tool, what: note.what, command, url: note.url };
}
