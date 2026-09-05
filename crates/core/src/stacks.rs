use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Result};
use serde::Serialize;

const LONGEST_NAME: usize = 64;
const A_STEP: Duration = Duration::from_secs(300);
const A_QUESTION: Duration = Duration::from_secs(10);

/// Where a version comes from when a starter is asked what it is today.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Registry {
    Npm,
    Crates,
}

/// How a finished project is checked for known vulnerabilities.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Audit {
    Npm,
    Cargo,
    Pip,
    Go,
}

impl Audit {
    pub fn tool(self) -> &'static str {
        match self {
            Audit::Npm => "npm",
            Audit::Cargo => "cargo-audit",
            Audit::Pip => "pip-audit",
            Audit::Go => "govulncheck",
        }
    }

    fn argv(self) -> &'static [&'static str] {
        match self {
            Audit::Npm => &["audit", "--json"],
            Audit::Cargo => &["audit", "--quiet"],
            Audit::Pip => &["--strict"],
            Audit::Go => &["./..."],
        }
    }

    fn command(self) -> &'static str {
        match self {
            Audit::Cargo => "cargo",
            other => other.tool(),
        }
    }

    /// How this auditor is asked whether it is here.
    ///
    /// `cargo --version` answers for cargo, not for cargo-audit, and saying an
    /// audit will run when it cannot is worse than saying nothing will.
    fn probe(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Audit::Npm => ("npm", &["--version"]),
            Audit::Cargo => ("cargo", &["audit", "--version"]),
            Audit::Pip => ("pip-audit", &["--version"]),
            Audit::Go => ("govulncheck", &["-version"]),
        }
    }

    pub async fn here(self) -> bool {
        let (tool, argv) = self.probe();
        answers(tool, argv).await
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Step {
    pub tool: &'static str,
    /// Everything after the tool. `{name}` is the only placeholder there is, and
    /// it is only ever filled with a name `valid_name` has already accepted.
    pub argv: &'static [&'static str],
    /// Run inside the new project rather than the folder above it. A starter
    /// whose first step is inside is one whose folder Agentland makes itself.
    pub inside: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Starter {
    pub id: &'static str,
    pub label: &'static str,
    /// What it is for, in one line.
    pub what: &'static str,
    /// Why this one rather than the obvious alternative.
    pub why: &'static str,
    pub steps: &'static [Step],
    /// Written into the project once the steps have run, so that what lands is
    /// something that runs rather than an empty module. Only files with no
    /// version in them: a version this repository writes down is one that goes
    /// stale the week after.
    pub files: &'static [(&'static str, &'static str)],
    /// The package whose version says what this stack is today. Asked of the
    /// same tool that will do the installing, so what is shown is what would
    /// land — never a number pinned in this file.
    pub headline: Option<(Registry, &'static str)>,
    pub audit: Option<Audit>,
}

impl Starter {
    /// Every tool this starter cannot run without.
    pub fn needs(&self) -> Vec<&'static str> {
        let mut tools: Vec<&'static str> = self.steps.iter().map(|step| step.tool).collect();
        tools.dedup();
        tools
    }
}

/// Something put on top of a starter, for the parts of a project that are
/// nobody's idea of a good afternoon and everybody's idea of a breach.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Extra {
    pub id: &'static str,
    pub label: &'static str,
    pub what: &'static str,
    pub why: &'static str,
    /// The starters this fits. A session has to be kept somewhere, so this is
    /// offered on the starters that have a server and not on the ones that are
    /// a folder of files a browser downloads.
    pub fits: &'static [&'static str],
    /// Run inside the project, once the starter has finished with it.
    pub steps: &'static [Step],
    pub files: &'static [(&'static str, &'static str)],
    /// What the project needs in its environment. A name marked secret is
    /// generated here — 32 bytes of `/dev/urandom` — and the rest are written
    /// empty, because the ones that come from another company's dashboard are
    /// not Agentland's to invent.
    pub env: &'static [(&'static str, bool)],
    pub env_file: &'static str,
    /// Patterns this makes sure the project's .gitignore carries. A database
    /// file is not source, and a generated client is not either.
    pub ignore: &'static [&'static str],
    /// A package whose installed version fills `{version}` in the steps below.
    ///
    /// Two halves of one tool have to be the same version, and a tag is not a
    /// promise that they are: `prisma@latest` is a release candidate today while
    /// `@prisma/client@latest` is the stable line behind it, so installing both
    /// by name gives a CLI a major version ahead of the client it drives. The
    /// version is read out of what actually landed in node_modules, which is the
    /// only number that cannot drift.
    pub lockstep: Option<&'static str>,
    pub headline: Option<(Registry, &'static str)>,
}

impl Extra {
    pub fn fits_starter(&self, starter_id: &str) -> bool {
        self.fits.iter().any(|held| *held == starter_id)
    }
}

const AXUM_MAIN: &str = r#"use axum::{routing::get, Router};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new().route("/", get(|| async { "it runs" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
"#;

const FASTAPI_APP: &str = r#"from fastapi import FastAPI

app = FastAPI()


@app.get("/")
async def root() -> dict[str, str]:
    return {"status": "it runs"}
"#;

const GO_MAIN: &str = r#"package main

import (
	"log"
	"net/http"
	"time"
)

func main() {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /", func(writer http.ResponseWriter, request *http.Request) {
		writer.Write([]byte("it runs"))
	})

	server := &http.Server{
		Addr:              "127.0.0.1:3000",
		Handler:           mux,
		ReadHeaderTimeout: 5 * time.Second,
	}

	log.Println("listening on http://127.0.0.1:3000")
	log.Fatal(server.ListenAndServe())
}
"#;

/// Go writes no .gitignore of its own, and a test or a coverage run leaves
/// files beside the source. Nothing else here needs one: cargo, uv and every
/// npm template bring their own.
const GO_IGNORE: &str = "*.exe\n*.dll\n*.so\n*.dylib\n*.test\n*.out\n";

pub const CATALOG: &[Starter] = &[
    Starter {
        id: "vite-react-ts",
        label: "Vite · React · TypeScript",
        what: "a browser app, and the fastest edit-to-pixel loop there is",
        why: "Vite serves modules the browser already understands, so a cold start is a second and a save is a frame. TypeScript is what stops half of what a crew of agents would otherwise get wrong.",
        steps: &[
            Step {
                tool: "npm",
                argv: &["create", "vite@latest", "{name}", "--", "--template", "react-ts"],
                inside: false,
            },
            Step {
                tool: "npm",
                argv: &["install"],
                inside: true,
            },
        ],
        files: &[],
        headline: Some((Registry::Npm, "vite")),
        audit: Some(Audit::Npm),
    },
    Starter {
        id: "next",
        label: "Next.js · TypeScript · Tailwind",
        what: "a site that renders on the server, with routing and a build already decided",
        why: "Server components mean the browser is sent markup instead of a framework, which is the difference that shows on a phone. Everything a page needs is decided for you, which is what makes it fast to start and hard to leave.",
        steps: &[Step {
            tool: "npm",
            argv: &[
                "create",
                "next-app@latest",
                "{name}",
                "--",
                "--ts",
                "--app",
                "--eslint",
                "--tailwind",
                "--src-dir",
                "--import-alias",
                "@/*",
                "--use-npm",
                "--yes",
            ],
            inside: false,
        }],
        files: &[],
        headline: Some((Registry::Npm, "next")),
        audit: Some(Audit::Npm),
    },
    Starter {
        id: "axum",
        label: "Rust · Axum · Tokio",
        what: "an HTTP service that holds its throughput under load",
        why: "No garbage collector, so the worst request is close to the median one — which is the number a service is actually judged on. The compiler refuses the memory bugs that make a service a security problem.",
        steps: &[
            Step {
                tool: "cargo",
                argv: &["new", "{name}"],
                inside: false,
            },
            Step {
                tool: "cargo",
                argv: &["add", "axum"],
                inside: true,
            },
            Step {
                tool: "cargo",
                argv: &["add", "tokio", "--features", "rt-multi-thread,macros,net"],
                inside: true,
            },
        ],
        files: &[("src/main.rs", AXUM_MAIN)],
        headline: Some((Registry::Crates, "axum")),
        audit: Some(Audit::Cargo),
    },
    Starter {
        id: "fastapi-uv",
        label: "Python · FastAPI · uv",
        what: "an API in the language the machine-learning half of the world writes in",
        why: "uv resolves and installs in seconds where pip takes minutes, and it writes a lockfile — which is what makes a Python project reproducible, and the only thing that makes it auditable.",
        steps: &[
            Step {
                tool: "uv",
                argv: &["init", "{name}"],
                inside: false,
            },
            Step {
                tool: "uv",
                argv: &["add", "fastapi", "uvicorn[standard]"],
                inside: true,
            },
        ],
        files: &[("app.py", FASTAPI_APP)],
        headline: None,
        audit: Some(Audit::Pip),
    },
    Starter {
        id: "go-http",
        label: "Go · the standard library",
        what: "a service with no dependencies at all",
        why: "The router, the server and the TLS are in the standard library, so there is no supply chain to audit and nothing to update but Go itself. It compiles to one binary that starts in milliseconds.",
        steps: &[Step {
            tool: "go",
            argv: &["mod", "init", "{name}"],
            inside: true,
        }],
        files: &[("main.go", GO_MAIN), (".gitignore", GO_IGNORE)],
        headline: None,
        audit: Some(Audit::Go),
    },
];

const AUTH_CONFIG: &str = r#"import NextAuth from "next-auth"
import GitHub from "next-auth/providers/github"

export const { handlers, signIn, signOut, auth } = NextAuth({
    providers: [GitHub],
})
"#;

const AUTH_ROUTE: &str = r#"import { handlers } from "@/auth"

export const { GET, POST } = handlers
"#;

const PRISMA_CLIENT: &str = r#"import { PrismaBetterSqlite3 } from "@prisma/adapter-better-sqlite3"

import { PrismaClient } from "@/generated/prisma/client"

const adapter = new PrismaBetterSqlite3({ url: process.env.DATABASE_URL ?? "file:./dev.db" })

const kept = globalThis as unknown as { prisma?: PrismaClient }

export const prisma = kept.prisma ?? new PrismaClient({ adapter })

if (process.env.NODE_ENV !== "production") {
    kept.prisma = prisma
}
"#;

pub const EXTRAS: &[Extra] = &[Extra {
    id: "auth-js",
    label: "Auth.js",
    what: "sign-in, sessions and a provider you do not write yourself",
    why: "Authentication is the one part of a project where writing it yourself is the wrong answer, and Auth.js is what the Next.js documentation itself points at. The version installed is v5, which is a beta and is said so plainly: it is the line the App Router is documented against, while the stable v4 predates it. The secret is generated here from /dev/urandom rather than left as a placeholder somebody ships.",
    fits: &["next"],
    steps: &[Step {
        tool: "npm",
        argv: &["install", "next-auth@beta"],
        inside: true,
    }],
    files: &[
        ("src/auth.ts", AUTH_CONFIG),
        ("src/app/api/auth/[...nextauth]/route.ts", AUTH_ROUTE),
    ],
    env: &[
        ("AUTH_SECRET", true),
        ("AUTH_GITHUB_ID", false),
        ("AUTH_GITHUB_SECRET", false),
    ],
    env_file: ".env.local",
    ignore: &[],
    lockstep: None,
    headline: Some((Registry::Npm, "next-auth@beta")),
},
Extra {
    id: "prisma",
    label: "Prisma",
    what: "a typed database client, and migrations that are files in the repository",
    why: "A query you got wrong stops compiling instead of returning the wrong row at three in the morning, and every schema change is a file somebody reviews rather than something that happened to a server. It starts on SQLite through a driver adapter, which needs nothing running — swapping the adapter is what moves it to Postgres once you know what you are deploying onto. The CLI is held to the version of the client that actually landed rather than to a tag, because prisma@latest is a release candidate today while the client behind it is not. Prisma also installs its own agent skills into the project, which this crew reads.",
    fits: &["next"],
    steps: &[
        Step {
            tool: "npm",
            argv: &["install", "@prisma/client"],
            inside: true,
        },
        Step {
            tool: "npm",
            argv: &["install", "--save-dev", "prisma@{version}", "dotenv"],
            inside: true,
        },
        Step {
            tool: "npm",
            argv: &["install", "@prisma/adapter-better-sqlite3@{version}"],
            inside: true,
        },
        Step {
            tool: "npx",
            argv: &["prisma", "init", "--datasource-provider", "sqlite"],
            inside: true,
        },
        Step {
            tool: "npx",
            argv: &["prisma", "generate"],
            inside: true,
        },
    ],
    files: &[("src/lib/prisma.ts", PRISMA_CLIENT)],
    env: &[],
    env_file: ".env",
    ignore: &["*.db", "*.db-journal"],
    lockstep: Some("@prisma/client"),
    headline: Some((Registry::Npm, "@prisma/client")),
}];

/// Whether a path a catalog entry writes stays inside the project.
///
/// The check is on path components, not on whether the text has two dots in it:
/// `src/app/api/auth/[...nextauth]/route.ts` is a folder name Next.js requires,
/// and reading it as an escape refuses the one file Auth.js cannot work without.
pub fn stays_inside(path: &str) -> bool {
    !path.starts_with('/')
        && Path::new(path)
            .components()
            .all(|piece| matches!(piece, std::path::Component::Normal(_)))
}

pub fn starter(id: &str) -> Option<&'static Starter> {
    CATALOG.iter().find(|entry| entry.id == id)
}

pub fn extra(id: &str) -> Option<&'static Extra> {
    EXTRAS.iter().find(|entry| entry.id == id)
}

pub fn extras_for(starter_id: &str) -> Vec<&'static Extra> {
    EXTRAS
        .iter()
        .filter(|entry| entry.fits_starter(starter_id))
        .collect()
}

/// Whether a name is safe to hand to a scaffolder as an argument.
///
/// Everything below is one command away from being run: a name that starts with
/// a dash is read as a flag, a name with a slash or `..` in it writes outside
/// the folder the person picked, and a name with a space in it becomes two
/// arguments at the first tool that forgets to quote. None of that is worth
/// being clever about, so the answer is a short allowlist.
pub fn valid_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("a project needs a name");
    }

    if name.len() > LONGEST_NAME {
        bail!("a project name is at most {LONGEST_NAME} characters");
    }

    if !name
        .chars()
        .next()
        .map(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        .unwrap_or(false)
    {
        bail!("a project name starts with a lowercase letter or a digit");
    }

    let allowed = |character: char| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, '-' | '_' | '.')
    };

    if let Some(bad) = name.chars().find(|character| !allowed(*character)) {
        bail!("a project name has no {bad:?} in it — lowercase letters, digits, dash, underscore and dot");
    }

    if name.contains("..") {
        bail!("a project name has no \"..\" in it");
    }

    Ok(())
}

pub fn fill(argv: &[&str], name: &str) -> Vec<String> {
    argv.iter()
        .map(|piece| piece.replace("{name}", name))
        .collect()
}

/// What `npm audit --json` found, in one line.
pub fn npm_audit_summary(json: &str) -> String {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json) else {
        return "npm audit said nothing this can read".to_owned();
    };

    let counts = parsed
        .get("metadata")
        .and_then(|metadata| metadata.get("vulnerabilities"));

    let Some(counts) = counts else {
        return "npm audit said nothing this can read".to_owned();
    };

    let at = |level: &str| counts.get(level).and_then(serde_json::Value::as_u64).unwrap_or(0);
    let total = at("critical") + at("high") + at("moderate") + at("low") + at("info");

    if total == 0 {
        return "npm audit: nothing known against it".to_owned();
    }

    let loud = at("critical") + at("high");
    if loud > 0 {
        format!(
            "npm audit: {loud} high or critical, {total} in all — worth reading before you build on it"
        )
    } else {
        format!("npm audit: {total} low or moderate, none high")
    }
}

/// The version in a line of `cargo search` output: `axum = "0.8.7"    # …`.
pub fn crates_version(output: &str, package: &str) -> Option<String> {
    output
        .lines()
        .find(|line| line.trim_start().starts_with(&format!("{package} = ")))
        .and_then(|line| line.split('"').nth(1))
        .map(str::to_owned)
}

async fn run(tool: &str, argv: &[String], cwd: &Path, patience: Duration) -> Result<std::process::Output> {
    let mut command = crate::exec::tokio_command(tool);
    command
        .args(argv)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

    match tokio::time::timeout(patience, command.output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => bail!("{tool} could not be run: {error}"),
        Err(_) => bail!("{tool} took longer than {} seconds", patience.as_secs()),
    }
}

fn last_words(output: &std::process::Output) -> String {
    let said = if output.stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout)
    } else {
        String::from_utf8_lossy(&output.stderr)
    };

    said.lines()
        .filter(|line| !line.trim().is_empty())
        .last()
        .unwrap_or("it said nothing")
        .trim()
        .to_owned()
}

/// How a tool is asked whether it is there.
///
/// `--version` for most of them. Go answers `go version` and nothing else, and
/// probing it wrongly reported the whole Go starter as uninstallable — so the
/// ones that differ are named here rather than assumed.
pub fn version_argv(tool: &str) -> &'static [&'static str] {
    match tool {
        "go" => &["version"],
        _ => &["--version"],
    }
}

async fn answers(tool: &str, argv: &[&str]) -> bool {
    let argv: Vec<String> = argv.iter().map(|piece| (*piece).to_owned()).collect();

    run(tool, &argv, Path::new("."), A_QUESTION)
        .await
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Whether a tool is on PATH at all.
pub async fn installed(tool: &str) -> bool {
    answers(tool, version_argv(tool)).await
}

/// What a starter's headline package is at this moment, asked of the tool that
/// would install it. None when the tool is not here or the network is not.
pub async fn headline_version(starter: &Starter) -> Option<String> {
    version_of(starter.headline).await
}

/// What a package is today, asked of the tool that would install it.
///
/// The package may carry a tag — `next-auth@beta` — and then the answer is the
/// version behind that tag, not the one behind `latest`. Showing 4.24 for an
/// install of `@beta` would be a number that is true of nothing.
pub async fn version_of(headline: Option<(Registry, &str)>) -> Option<String> {
    let (registry, package) = headline?;
    let here = Path::new(".");

    match registry {
        Registry::Npm => {
            let output = run(
                "npm",
                &["view".to_owned(), package.to_owned(), "version".to_owned()],
                here,
                A_QUESTION,
            )
            .await
            .ok()?;

            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
                .filter(|version| !version.is_empty())
        }
        Registry::Crates => {
            let output = run(
                "cargo",
                &[
                    "search".to_owned(),
                    package.to_owned(),
                    "--limit".to_owned(),
                    "1".to_owned(),
                ],
                here,
                A_QUESTION,
            )
            .await
            .ok()?;

            output
                .status
                .success()
                .then(|| crates_version(&String::from_utf8_lossy(&output.stdout), package))
                .flatten()
        }
    }
}

fn write_into(project: &Path, where_it_goes: &str, body: &str) -> Result<()> {
    if !stays_inside(where_it_goes) {
        bail!("{where_it_goes} is not inside the project");
    }

    let file = project.join(where_it_goes);
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(&file, body)?;

    Ok(())
}

pub struct Made {
    pub path: PathBuf,
    pub did: Vec<String>,
}

/// Make a project of this kind, in a folder of this name, under this one.
///
/// The folder must not exist yet. Nothing here is composed into a shell line:
/// the tool and its arguments come from the catalog above, and the only thing
/// the caller contributes is a name that has already been through `valid_name`.
pub async fn scaffold(starter: &Starter, parent: &Path, name: &str) -> Result<Made> {
    valid_name(name)?;

    if !parent.is_dir() {
        bail!("{} is not a folder", parent.display());
    }

    let parent = crate::exec::plain(parent.canonicalize()?);
    let project = parent.join(name);
    if project.exists() {
        bail!("{} is already there", project.display());
    }

    let mut did = Vec::new();

    for step in starter.steps {
        let argv = fill(step.argv, name);
        let cwd = if step.inside {
            std::fs::create_dir_all(&project)?;
            project.as_path()
        } else {
            parent.as_path()
        };

        let output = run(step.tool, &argv, cwd, A_STEP).await?;
        if !output.status.success() {
            bail!("{} {} failed: {}", step.tool, argv.join(" "), last_words(&output));
        }
    }

    if !project.is_dir() {
        bail!("{} made no {}", starter.label, project.display());
    }

    did.push(format!("made {name} with {}", starter.label));

    for (where_it_goes, body) in starter.files {
        write_into(&project, where_it_goes, body)?;
    }

    if !starter.files.is_empty() {
        did.push(format!("wrote something that runs into {name}"));
    }

    Ok(Made { path: project, did })
}

/// Whether a `.gitignore` pattern covers a file name. Only `*` is understood,
/// which is every pattern this has to read.
fn glob_matches(pattern: &str, name: &str) -> bool {
    let mut rest = name;
    let mut pieces = pattern.split('*');

    let Some(first) = pieces.next() else {
        return false;
    };

    let Some(after) = rest.strip_prefix(first) else {
        return false;
    };
    rest = after;

    let mut last: Option<&str> = None;
    for piece in pieces {
        last = Some(piece);
        if piece.is_empty() {
            continue;
        }
        let Some(at) = rest.find(piece) else {
            return false;
        };
        rest = &rest[at + piece.len()..];
    }

    match last {
        // A pattern with no `*` in it has to have been consumed exactly.
        None => rest.is_empty(),
        // One ending in `*` may leave anything behind; one ending in text has
        // to have matched that text at the end.
        Some(piece) => piece.is_empty() || rest.is_empty(),
    }
}

/// Whether a `.gitignore` already keeps this file out of a commit.
pub fn ignores(gitignore: &str, file: &str) -> bool {
    gitignore
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
        .any(|line| glob_matches(line.trim_start_matches('/'), file))
}

/// Make sure a file with a secret in it cannot be committed.
///
/// Every template here brings a `.gitignore` that already covers `.env.local`,
/// and the whole point of checking is the day one of them stops. Writing a
/// generated secret into a folder that would commit it is the one failure in
/// this file that cannot be taken back once it is pushed.
pub fn keep_out_of_git(project: &Path, file: &str) -> Result<bool> {
    let path = project.join(".gitignore");
    let held = std::fs::read_to_string(&path).unwrap_or_default();

    if ignores(&held, file) {
        return Ok(false);
    }

    let mut next = held;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(file);
    next.push('\n');
    std::fs::write(&path, next)?;

    Ok(true)
}

/// Thirty-two bytes nobody can guess, as hex.
///
/// A session secret this cannot generate is not one it should invent: there is
/// no fallback to the clock or the process id, because a secret derived from
/// those is a secret an attacker derives too.
fn a_secret() -> Result<String> {
    use std::io::Read;

    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| anyhow::anyhow!("no secure randomness on this machine: {error}"))?;

    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The lines an extra adds to an environment file.
pub fn env_lines(env: &[(&str, bool)], secret: impl Fn() -> Result<String>) -> Result<String> {
    let mut out = String::new();

    for (name, is_secret) in env {
        let value = if *is_secret { secret()? } else { String::new() };
        out.push_str(&format!("{name}={value}\n"));
    }

    Ok(out)
}

/// The version in a package's own `package.json`.
pub fn version_in_manifest(manifest: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(manifest)
        .ok()?
        .get("version")?
        .as_str()
        .map(str::to_owned)
}

fn installed_version(project: &Path, package: &str) -> Option<String> {
    let manifest = project.join("node_modules").join(package).join("package.json");
    version_in_manifest(&std::fs::read_to_string(manifest).ok()?)
}

fn swap(argv: &[&str], key: &str, value: &str) -> Vec<String> {
    argv.iter().map(|piece| piece.replace(key, value)).collect()
}

/// Put an extra on a project that has already been made.
pub async fn add(extra: &Extra, project: &Path) -> Result<Vec<String>> {
    let mut did = Vec::new();

    for step in extra.steps {
        let argv: Vec<String> = if step.argv.iter().any(|piece| piece.contains("{version}")) {
            let package = extra
                .lockstep
                .ok_or_else(|| anyhow::anyhow!("{} asks for a version nothing pins", extra.id))?;

            let version = installed_version(project, package).ok_or_else(|| {
                anyhow::anyhow!("{package} is not installed, so nothing can be kept in step with it")
            })?;

            swap(step.argv, "{version}", &version)
        } else {
            step.argv.iter().map(|piece| (*piece).to_owned()).collect()
        };

        let output = run(step.tool, &argv, project, A_STEP).await?;
        if !output.status.success() {
            bail!("{} {} failed: {}", step.tool, argv.join(" "), last_words(&output));
        }
    }

    for (where_it_goes, body) in extra.files {
        write_into(project, where_it_goes, body)?;
    }

    did.push(format!("added {} to the project", extra.label));

    let mut taught = Vec::new();
    for pattern in extra.ignore {
        if keep_out_of_git(project, pattern)? {
            taught.push(*pattern);
        }
    }

    if !taught.is_empty() {
        did.push(format!("taught .gitignore about {}", taught.join(" and ")));
    }

    if !extra.env.is_empty() {
        let appended = keep_out_of_git(project, extra.env_file)?;
        let lines = env_lines(extra.env, a_secret)?;

        let path = project.join(extra.env_file);
        let mut held = std::fs::read_to_string(&path).unwrap_or_default();
        if !held.is_empty() && !held.ends_with('\n') {
            held.push('\n');
        }
        held.push_str(&lines);
        std::fs::write(&path, held)?;

        let secrets: Vec<&str> = extra
            .env
            .iter()
            .filter(|(_, is_secret)| *is_secret)
            .map(|(name, _)| *name)
            .collect();

        did.push(format!(
            "generated {} into {}{}",
            secrets.join(" and "),
            extra.env_file,
            if appended {
                ", and taught .gitignore to keep it out of the commit"
            } else {
                ", which .gitignore already keeps out of the commit"
            }
        ));

        let waiting: Vec<&str> = extra
            .env
            .iter()
            .filter(|(_, is_secret)| !*is_secret)
            .map(|(name, _)| *name)
            .collect();

        if !waiting.is_empty() {
            did.push(format!(
                "{} {} waiting in {} — {} from the provider, not from here",
                waiting.join(", "),
                if waiting.len() == 1 { "is" } else { "are" },
                extra.env_file,
                if waiting.len() == 1 { "it comes" } else { "they come" }
            ));
        }
    }

    Ok(did)
}

#[derive(Clone, Debug, Serialize)]
pub struct Vetting {
    pub tool: &'static str,
    /// What was found, or why nothing was looked for.
    pub summary: String,
    pub ran: bool,
}

/// Ask the ecosystem's own auditor what is known against what was just installed.
///
/// A project is not started until somebody has looked, and the tool that knows
/// is the ecosystem's own. When it is not installed that is said plainly rather
/// than passed off as a clean result — an audit nobody ran is not a pass.
pub async fn vet(kind: Audit, project: &Path) -> Vetting {
    let tool = kind.tool();

    if !kind.here().await {
        return Vetting {
            tool,
            summary: format!("{tool} is not installed, so nothing was checked"),
            ran: false,
        };
    }

    let argv: Vec<String> = kind.argv().iter().map(|piece| (*piece).to_owned()).collect();
    let Ok(output) = run(kind.command(), &argv, project, A_STEP).await else {
        return Vetting {
            tool,
            summary: format!("{tool} did not finish"),
            ran: false,
        };
    };

    let summary = match kind {
        Audit::Npm => npm_audit_summary(&String::from_utf8_lossy(&output.stdout)),
        _ if output.status.success() => format!("{tool}: nothing known against it"),
        _ => format!("{tool}: {}", last_words(&output)),
    };

    Vetting {
        tool,
        summary,
        ran: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_would_become_a_flag_is_refused() {
        assert!(valid_name("-rf").is_err());
        assert!(valid_name("--force").is_err());
    }

    #[test]
    fn a_name_that_would_write_outside_the_folder_is_refused() {
        assert!(valid_name("../elsewhere").is_err());
        assert!(valid_name("a/b").is_err());
        assert!(valid_name("..").is_err());
    }

    #[test]
    fn a_name_that_would_become_two_arguments_is_refused() {
        assert!(valid_name("my app").is_err());
        assert!(valid_name("app;rm").is_err());
        assert!(valid_name("app$(id)").is_err());
        assert!(valid_name("app\nnext").is_err());
    }

    #[test]
    fn an_ordinary_name_is_taken() {
        for name in ["svc-demo", "web", "app_2", "next.thing", "3d"] {
            assert!(valid_name(name).is_ok(), "{name} is a name people use");
        }
    }

    #[test]
    fn a_name_reaches_the_arguments_and_nothing_else_does() {
        let filled = fill(&["create", "vite@latest", "{name}", "--", "--template", "react-ts"], "web");

        assert_eq!(filled[2], "web");
        assert_eq!(filled[0], "create");
        assert_eq!(filled.len(), 6);
    }

    #[test]
    fn every_starter_in_the_catalog_can_be_run() {
        for starter in CATALOG {
            assert!(!starter.steps.is_empty(), "{} does nothing", starter.id);
            assert!(!starter.needs().is_empty(), "{} needs no tool", starter.id);
            assert!(
                starter.steps.iter().any(|step| !step.inside) || starter.steps[0].inside,
                "{} makes no folder",
                starter.id
            );
            assert!(
                starter.files.iter().all(|(path, _)| stays_inside(path)),
                "{} writes outside its project",
                starter.id
            );
        }
    }

    #[test]
    fn a_clean_audit_and_a_loud_one_read_differently() {
        let clean = r#"{"metadata":{"vulnerabilities":{"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#;
        assert_eq!(npm_audit_summary(clean), "npm audit: nothing known against it");

        let loud = r#"{"metadata":{"vulnerabilities":{"info":0,"low":1,"moderate":0,"high":2,"critical":1}}}"#;
        let said = npm_audit_summary(loud);
        assert!(said.contains("3 high or critical"), "{said}");
        assert!(said.contains("4 in all"), "{said}");

        let quiet = r#"{"metadata":{"vulnerabilities":{"info":0,"low":2,"moderate":1,"high":0,"critical":0}}}"#;
        assert_eq!(npm_audit_summary(quiet), "npm audit: 3 low or moderate, none high");
    }

    #[test]
    fn an_audit_nobody_could_read_is_not_called_clean() {
        for said in ["", "not json at all", "{}"] {
            assert!(
                !npm_audit_summary(said).contains("nothing known against it"),
                "{said:?} was read as a pass"
            );
        }
    }

    #[test]
    fn a_tool_that_answers_differently_is_asked_differently() {
        assert_eq!(version_argv("go"), ["version"]);
        assert_eq!(version_argv("npm"), ["--version"]);
        assert_eq!(version_argv("cargo"), ["--version"]);
    }

    #[test]
    fn an_auditor_is_probed_for_itself_and_not_for_what_carries_it() {
        assert_eq!(Audit::Cargo.probe(), ("cargo", &["audit", "--version"][..]));
        assert_ne!(Audit::Cargo.probe().1, &["--version"][..]);
    }

    #[test]
    fn an_extra_is_only_offered_where_it_can_work() {
        let auth = extra("auth-js").expect("Auth.js is in the catalog");

        assert!(auth.fits_starter("next"));
        assert!(!auth.fits_starter("vite-react-ts"), "a static bundle keeps no session");
        assert!(!auth.fits_starter("go-http"));

        let on_next: Vec<&str> = extras_for("next").into_iter().map(|held| held.id).collect();
        assert!(on_next.contains(&"auth-js") && on_next.contains(&"prisma"), "{on_next:?}");
        assert!(extras_for("go-http").is_empty());
    }

    #[test]
    fn every_extra_fits_a_starter_that_exists() {
        for held in EXTRAS {
            assert!(!held.fits.is_empty(), "{} fits nothing", held.id);
            for id in held.fits {
                assert!(starter(id).is_some(), "{} fits {id}, which is not a starter", held.id);
            }
            assert!(
                held.files.iter().all(|(path, _)| stays_inside(path)),
                "{} writes outside its project",
                held.id
            );
        }
    }

    #[test]
    fn a_route_next_requires_is_not_mistaken_for_an_escape() {
        assert!(stays_inside("src/app/api/auth/[...nextauth]/route.ts"));
        assert!(stays_inside("main.go"));

        assert!(!stays_inside("../elsewhere.ts"));
        assert!(!stays_inside("/etc/passwd"));
        assert!(!stays_inside("src/../../out.ts"));
    }

    #[test]
    fn what_prisma_writes_is_reachable_from_where_it_writes_it() {
        // `prisma init` reads the project and puts its client under `src/` when
        // there is one, and beside it when there is not — measured, after a
        // hardcoded relative import compiled in a bare folder and did not in a
        // Next project. The `@/` alias is only the right answer while every
        // starter this fits is scaffolded with a src directory, so that is the
        // thing worth failing on.
        let prisma = extra("prisma").expect("Prisma is in the catalog");
        assert!(prisma
            .files
            .iter()
            .any(|(_, body)| body.contains("@/generated/prisma")));

        for id in prisma.fits {
            let held = starter(id).expect("a starter it fits");
            assert!(
                held.steps
                    .iter()
                    .any(|step| step.argv.contains(&"--src-dir")),
                "{id} has no src directory, so @/ does not reach the generated client"
            );
        }
    }

    #[test]
    fn the_gitignore_a_template_brings_is_read_rather_than_assumed() {
        assert!(ignores(".env*.local\nnode_modules\n", ".env.local"));
        assert!(ignores("/.env*\n", ".env.local"));
        assert!(ignores(".env.local\n", ".env.local"));
        assert!(ignores("*.local\n", ".env.local"));
    }

    #[test]
    fn a_gitignore_that_does_not_cover_the_secret_is_not_read_as_if_it_did() {
        assert!(!ignores(".env\nnode_modules\n", ".env.local"));
        assert!(!ignores("# .env.local\n", ".env.local"), "a comment ignores nothing");
        assert!(!ignores("!.env.local\n", ".env.local"), "an unignore ignores nothing");
        assert!(!ignores("", ".env.local"));
        assert!(!ignores("env.local\n", ".env.local"));
    }

    #[test]
    fn a_generated_secret_is_written_and_a_borrowed_one_is_left_empty() {
        let written = env_lines(
            &[("AUTH_SECRET", true), ("AUTH_GITHUB_ID", false)],
            || Ok("beef".to_owned()),
        )
        .expect("a secret");

        assert_eq!(written, "AUTH_SECRET=beef\nAUTH_GITHUB_ID=\n");
    }

    #[test]
    fn a_secret_that_cannot_be_generated_is_never_invented() {
        let refused = env_lines(&[("AUTH_SECRET", true)], || bail!("no randomness"));

        assert!(refused.is_err(), "a guessable secret is worse than none");
    }

    #[test]
    fn the_secret_is_long_and_different_every_time() {
        let one = a_secret().expect("this machine has /dev/urandom");
        let two = a_secret().expect("this machine has /dev/urandom");

        assert_eq!(one.len(), 64, "32 bytes as hex");
        assert_ne!(one, two);
        assert!(one.chars().all(|character| character.is_ascii_hexdigit()));
    }

    #[test]
    fn the_two_halves_of_a_tool_are_pinned_to_what_actually_landed() {
        let manifest = r#"{"name":"@prisma/client","version":"7.10.0","main":"index.js"}"#;
        assert_eq!(version_in_manifest(manifest).as_deref(), Some("7.10.0"));

        let filled = swap(&["install", "--save-dev", "prisma@{version}", "dotenv"], "{version}", "7.10.0");
        assert_eq!(filled[2], "prisma@7.10.0");
        assert_eq!(filled[3], "dotenv", "nothing else is touched");
    }

    #[test]
    fn a_manifest_nobody_can_read_pins_nothing() {
        for said in ["", "not json", "{}", r#"{"version":7}"#] {
            assert_eq!(version_in_manifest(said), None, "{said:?} was read as a version");
        }
    }

    #[test]
    fn an_extra_that_asks_for_a_version_says_where_it_comes_from() {
        for held in EXTRAS {
            let asks = held
                .steps
                .iter()
                .any(|step| step.argv.iter().any(|piece| piece.contains("{version}")));

            assert_eq!(
                asks,
                held.lockstep.is_some(),
                "{} either asks for a version and pins it, or does neither",
                held.id
            );
        }
    }

    #[test]
    fn a_version_is_read_out_of_what_cargo_prints() {
        let output = "axum = \"0.8.7\"    # Web framework that focuses on ergonomics\n... and 84 crates more";

        assert_eq!(crates_version(output, "axum").as_deref(), Some("0.8.7"));
        assert_eq!(crates_version(output, "tokio"), None);
        assert_eq!(crates_version("", "axum"), None);
    }
}
