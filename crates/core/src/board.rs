use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// A file somebody put on a card: a screenshot of the bug, a design, a log.
///
/// The bytes live on disk under Agentland's own folder and the card carries
/// the path. An agent is a process on this machine reading a brief, and a path
/// in a brief is a file it opens — the same way a screenshot pasted into a
/// terminal reaches Claude Code. Nothing is inlined, so the board stays text.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Attachment {
    pub name: String,
    /// Absolute, so it reads the same from any worktree.
    pub path: String,
    pub kind: String,
    pub bytes: u64,
    #[serde(default)]
    pub at: u64,
    /// What a person drew on the picture, in the picture's own pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marks: Option<Marks>,
    /// The attachment this one was made from — a marked-up copy of a
    /// picture is derived from the picture. Derived files are not shown as
    /// files of their own, and go when their original goes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<String>,
}

impl Attachment {
    pub fn is_image(&self) -> bool {
        self.kind.starts_with("image/")
    }
}

/// One thing drawn on a picture.
///
/// `kind` is one of `box`, `arrow`, `pen`, `pin` and `label`; `points` are
/// in the picture's pixels, x then y — two corners for a box, from and to
/// for an arrow, the stroke for a pen, one point for a pin or a label. The
/// text is what the person said about it, if anything.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Mark {
    pub kind: String,
    #[serde(default)]
    pub points: Vec<[f64; 2]>,
    #[serde(default)]
    pub text: String,
}

/// Everything drawn on one picture, and the size it was drawn at.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Marks {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub marks: Vec<Mark>,
}

impl Marks {
    /// The marks in words, numbered as they are drawn on the marked copy.
    ///
    /// An agent reads the picture, but a box is a region and a region is
    /// coordinates: the words say where, and what the person meant by it.
    pub fn legend(&self) -> Vec<String> {
        self.marks
            .iter()
            .enumerate()
            .map(|(n, mark)| {
                let at = |index: usize| {
                    mark.points
                        .get(index)
                        .map(|point| format!("({}, {})", point[0].round(), point[1].round()))
                        .unwrap_or_else(|| "(?)".to_owned())
                };
                let place = match mark.kind.as_str() {
                    "box" => format!("box from {} to {}", at(0), at(1)),
                    "arrow" => format!("arrow from {} pointing at {}", at(0), at(1)),
                    "pen" => {
                        let (left, top, right, bottom) = bounds(&mark.points);
                        format!(
                            "freehand stroke within ({left}, {top}) to ({right}, {bottom})"
                        )
                    }
                    "pin" => format!("pin at {}", at(0)),
                    "label" => format!("label at {}", at(0)),
                    other => format!("{other} at {}", at(0)),
                };
                let said = mark.text.trim();
                if said.is_empty() {
                    format!("{}. {place}", n + 1)
                } else {
                    format!("{}. {place}: \"{said}\"", n + 1)
                }
            })
            .collect()
    }
}

fn bounds(points: &[[f64; 2]]) -> (f64, f64, f64, f64) {
    points.iter().fold(
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        |(left, top, right, bottom), point| {
            (
                left.min(point[0]).round(),
                top.min(point[1]).round(),
                right.max(point[0]).round(),
                bottom.max(point[1]).round(),
            )
        },
    )
}

/// How many bytes one attached file may be.
pub const MOST_ATTACHMENT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Column {
    Backlog,
    Assigned,
    Working,
    Review,
    /// Reviewed, green and mergeable — waiting on nothing but a person saying
    /// yes. The difference between this and `Review` is whose turn it is.
    Ready,
    Done,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Evidence {
    Commit { sha: String, subject: String },
    Diff { files: usize, insertions: u32, deletions: u32 },
    PullRequest { url: String },
    Note { text: String },
    /// What somebody who did not write the work made of it.
    Reviewed { verdict: String, summary: String },
    /// What an agent says it did when its turn on this card ended, and what the
    /// worktree looked like when it stopped. The one entry on a card that is a
    /// report rather than a remark.
    Finished {
        summary: String,
        #[serde(default)]
        files: usize,
        #[serde(default)]
        insertions: u32,
        #[serde(default)]
        deletions: u32,
    },
}

impl Evidence {
    /// Whether this is a record of work rather than a remark about it.
    ///
    /// A card carrying a record cannot be discarded by an agent. The two used to
    /// be the same thing, and a routing note — *"Nova is the free agent with the
    /// closest role"* — was enough to make a duplicate card undeletable, which
    /// is a guard protecting nothing at the cost of a job nobody could finish.
    pub fn is_a_record(&self) -> bool {
        !matches!(self, Evidence::Note { .. })
    }
}

/// A piece of evidence, and who put it there.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Entry {
    pub what: Evidence,
    /// An agent id, `the supervisor`, or whoever else wrote it down.
    pub by: String,
    /// Seconds. Zero for entries from before anyone recorded it.
    #[serde(default)]
    pub at: u64,
}

impl Entry {
    pub fn new(what: Evidence, by: &str, at: u64) -> Self {
        Self {
            what,
            by: by.to_owned(),
            at,
        }
    }
}

/// Entries were once bare evidence with nobody's name on them, and a board that
/// came back empty because the shape changed would be worse than one that never
/// learned who did what. Both shapes are read; only the new one is written.
impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D: serde::Deserializer<'de>>(reader: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Written {
            what: Evidence,
            #[serde(default)]
            by: String,
            #[serde(default)]
            at: u64,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            Attributed(Written),
            Bare(Evidence),
        }

        Ok(match Either::deserialize(reader)? {
            Either::Attributed(held) => Entry {
                what: held.what,
                by: if held.by.is_empty() {
                    "someone".to_owned()
                } else {
                    held.by
                },
                at: held.at,
            },
            Either::Bare(what) => Entry {
                what,
                by: "someone".to_owned(),
                at: 0,
            },
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub column: Column,
    pub repository_id: String,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub worktree: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub evidence: Vec<Entry>,
    /// When it was written down. Zero for cards from before anyone thought to
    /// record it — shown as "no date" rather than as the epoch.
    #[serde(default)]
    pub at: u64,
    /// Where it sits among the cards in its column, smallest first.
    ///
    /// A fraction rather than an index so a card dropped between two others
    /// takes the midpoint and nothing else has to be renumbered. Cards written
    /// before anyone could order them share zero and fall back to their id.
    #[serde(default)]
    pub position: f64,
    /// Files put on the card by a person: what the work looks like, or should.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

impl Task {
    /// What an agent is told when it is handed this card.
    ///
    /// The title, the body, and then every attached file by its path, so the
    /// agent reads the screenshot rather than being told there was one.
    pub fn brief(&self) -> String {
        let mut brief = format!("{}\n\n{}", self.title, self.body);

        let originals: Vec<&Attachment> = self
            .attachments
            .iter()
            .filter(|held| held.derived_from.is_none())
            .collect();

        if !originals.is_empty() {
            brief.push_str(
                "\n\nAttached to this card — open and read each of these before you start, they are part of the brief:",
            );
            for held in originals {
                brief.push_str(&format!("\n- {} ({}, {})", held.path, held.kind, sized(held.bytes)));

                let marked = self
                    .attachments
                    .iter()
                    .find(|other| other.derived_from.as_deref() == Some(held.name.as_str()));

                if let Some(marks) = held.marks.as_ref().filter(|marks| !marks.marks.is_empty()) {
                    match marked {
                        Some(copy) => brief.push_str(&format!(
                            "\n  A person marked this picture up. Read the marked copy, {}, where each mark is numbered; the picture is {}×{} pixels and the marks are:",
                            copy.path, marks.width, marks.height
                        )),
                        None => brief.push_str(&format!(
                            "\n  A person marked this picture up. It is {}×{} pixels and the marks, in its pixels, are:",
                            marks.width, marks.height
                        )),
                    }
                    for line in marks.legend() {
                        brief.push_str(&format!("\n    {line}"));
                    }
                    brief.push_str(
                        "\n  Every numbered mark is something the person is pointing at and is part of what this card asks for. Address each one, and say in your report what you did about each, by number.",
                    );
                }
            }
        }

        brief
    }
}

/// A size as a person would say it.
fn sized(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// A file name as it may be written under the card's folder.
///
/// Whatever arrived — a path, a name with slashes, dots leading somewhere —
/// becomes one plain name, because the folder it lands in is chosen here and
/// not by the sender.
pub fn safe_name(wanted: &str) -> String {
    let base = wanted
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches('.');

    let cleaned: String = base
        .chars()
        .map(|c| if c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();

    if cleaned.is_empty() {
        "file".to_owned()
    } else {
        cleaned
    }
}

/// A name no file in the folder has yet: the wanted one, or it with a number.
fn unclaimed(folder: &std::path::Path, wanted: &str) -> String {
    if !folder.join(wanted).exists() {
        return wanted.to_owned();
    }

    let (stem, extension) = match wanted.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, format!(".{extension}")),
        _ => (wanted, String::new()),
    };

    (2..)
        .map(|n| format!("{stem}-{n}{extension}"))
        .find(|name| !folder.join(name).exists())
        .expect("the numbers do not run out")
}

#[derive(Clone, Debug, Deserialize)]
pub struct EditTask {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateTask {
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub repository_id: String,
    /// The worktree this work belongs in, when it belongs in one. A step that
    /// commits to a branch can only be done where that branch is checked out.
    #[serde(default)]
    pub worktree: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MoveTask {
    pub column: Column,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    tasks: BTreeMap<String, Task>,
    #[serde(default)]
    next_number: u32,
}

pub struct Board {
    state: Mutex<State>,
    data_dir: PathBuf,
}

impl Board {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = crate::exec::settled(&data_dir);
        let state = crate::db::load_state(&data_dir, "board");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "board", state);
    }

    pub fn list(&self) -> Vec<Task> {
        self.state.lock().tasks.values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<Task> {
        self.state.lock().tasks.get(id).cloned()
    }

    pub fn create(&self, request: CreateTask) -> Result<Task> {
        if request.title.trim().is_empty() {
            bail!("a task needs a title");
        }

        let mut state = self.state.lock();
        state.next_number += 1;
        let id = format!("t{}", state.next_number);

        let task = Task {
            id: id.clone(),
            title: request.title,
            body: request.body,
            column: Column::Backlog,
            repository_id: request.repository_id,
            assignee: None,
            worktree: request.worktree,
            branch: None,
            evidence: Vec::new(),
            at: now_secs(),
            position: state
                .tasks
                .values()
                .filter(|held| held.column == Column::Backlog)
                .map(|held| held.position)
                .fold(0.0_f64, f64::max)
                + 1.0,
            attachments: Vec::new(),
        };

        state.tasks.insert(id, task.clone());
        self.persist(&state);
        Ok(task)
    }

    /// Change what a card says.
    ///
    /// A card used to be written once: a typo in the brief, or a detail learned
    /// after the fact, meant deleting it and losing its history.
    pub fn edit(&self, id: &str, change: EditTask) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        if let Some(title) = change.title {
            if title.trim().is_empty() {
                bail!("a task needs a title");
            }
            task.title = title;
        }

        if let Some(body) = change.body {
            task.body = body;
        }

        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Where a card's files live.
    fn folder_of(&self, id: &str) -> PathBuf {
        self.data_dir.join("attachments").join(id)
    }

    /// Put a file on a card.
    ///
    /// A file derived from another — the marked copy of a picture — keeps the
    /// name it is given and replaces what was there under it: there is one
    /// marked copy of a picture, and it is the latest.
    pub fn attach_file(
        &self,
        id: &str,
        name: &str,
        kind: &str,
        bytes: &[u8],
        derived_from: Option<&str>,
    ) -> Result<Task> {
        if bytes.is_empty() {
            bail!("nothing arrived to attach");
        }
        if bytes.len() > MOST_ATTACHMENT_BYTES {
            bail!("{name} is too large to attach — {} is the most", sized(MOST_ATTACHMENT_BYTES as u64));
        }

        let mut state = self.state.lock();
        if !state.tasks.contains_key(id) {
            bail!("unknown task: {id}");
        }

        if let Some(original) = derived_from {
            let task = state.tasks.get(id).expect("checked above");
            if !task.attachments.iter().any(|held| held.name == original) {
                bail!("{id} has nothing called {original} to derive from");
            }
        }

        let folder = self.folder_of(id);
        fs::create_dir_all(&folder)?;
        let name = match derived_from {
            Some(_) => safe_name(name),
            None => unclaimed(&folder, &safe_name(name)),
        };
        let path = folder.join(&name);
        fs::write(&path, bytes)?;

        let kind = if kind.trim().is_empty() {
            "application/octet-stream"
        } else {
            kind.trim()
        };

        let task = state.tasks.get_mut(id).expect("checked above");
        if derived_from.is_some() {
            task.attachments.retain(|held| held.name != name);
        }
        task.attachments.push(Attachment {
            name,
            path: path.to_string_lossy().into_owned(),
            kind: kind.to_owned(),
            bytes: bytes.len() as u64,
            at: now_secs(),
            marks: None,
            derived_from: derived_from.map(str::to_owned),
        });

        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Write down what a person drew on a picture.
    pub fn set_marks(&self, id: &str, name: &str, marks: Marks) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        let held = task
            .attachments
            .iter_mut()
            .find(|held| held.name == name)
            .ok_or_else(|| anyhow!("{id} has nothing called {name} on it"))?;

        if !held.is_image() {
            bail!("{name} is not a picture, and marks go on pictures");
        }

        held.marks = if marks.marks.is_empty() { None } else { Some(marks) };

        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Take a file off a card, and off the disk.
    pub fn detach_file(&self, id: &str, name: &str) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        let seat = task
            .attachments
            .iter()
            .position(|held| held.name == name)
            .ok_or_else(|| anyhow!("{id} has nothing called {name} on it"))?;

        let gone = task.attachments.remove(seat);
        let _ = fs::remove_file(&gone.path);

        // What was made from it goes with it.
        let derived: Vec<Attachment> = task
            .attachments
            .iter()
            .filter(|held| held.derived_from.as_deref() == Some(name))
            .cloned()
            .collect();
        for copy in derived {
            let _ = fs::remove_file(&copy.path);
            task.attachments.retain(|held| held.name != copy.name);
        }

        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// The file behind an attachment, if the card has it.
    pub fn attachment(&self, id: &str, name: &str) -> Result<Attachment> {
        let state = self.state.lock();
        let task = state
            .tasks
            .get(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        task.attachments
            .iter()
            .find(|held| held.name == name)
            .cloned()
            .ok_or_else(|| anyhow!("{id} has nothing called {name} on it"))
    }

    /// Move a card to a column, at the bottom of it.
    ///
    /// A card kept the position it had in the column it left, so "done" read
    /// in the order cards had once sat in "backlog" rather than the order they
    /// finished in. Arriving at the bottom keeps a column in order of arrival;
    /// a card already in the column keeps its place, which is somebody's
    /// choice. Dropping between two cards is `place`.
    pub fn move_to(&self, id: &str, column: Column) -> Result<Task> {
        let mut state = self.state.lock();

        let bottom = state
            .tasks
            .values()
            .filter(|held| held.column == column && held.id != id)
            .map(|held| held.position)
            .fold(None, |most: Option<f64>, held| Some(most.map_or(held, |most| most.max(held))));

        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        if task.column != column {
            task.column = column;
            task.position = placed_between(bottom, None);
        }

        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    pub fn attach(&self, id: &str, evidence: Evidence, by: &str, at: u64) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;
        task.evidence.push(Entry::new(evidence, by, at));
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    pub fn record_assignment(
        &self,
        id: &str,
        assignee: &str,
        worktree: &str,
        branch: &str,
    ) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        task.assignee = Some(assignee.to_owned());
        // A card bound to a worktree keeps that binding: it says where the work
        // belongs, which outlives whoever is holding the card.
        task.worktree
            .get_or_insert_with(|| worktree.to_owned());
        task.branch = Some(branch.to_owned());
        task.column = Column::Working;
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Take a card back from whoever holds it.
    ///
    /// A card handed to the wrong agent used to be dead: dispatch refused it
    /// because it already belonged to someone, and the holder could not be told
    /// to let go, so the only way on was a fresh card and a lost history. The
    /// worktree it belongs in survives the release — that was a property of the
    /// work, not of the agent that was holding it.
    pub fn release(&self, id: &str) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        let held_by = task.assignee.take();
        task.branch = None;
        task.column = Column::Backlog;

        if let Some(who) = held_by {
            task.evidence.push(Entry::new(
                Evidence::Note {
                    text: format!("taken back from {who}"),
                },
                "the board",
                now_secs(),
            ));
        }

        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Say which branch a card's work is on.
    pub fn record_branch(&self, id: &str, branch: &str) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        task.branch = Some(branch.to_owned());
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Say which worktree a card belongs in, before anyone is handed it.
    pub fn bind_to_worktree(&self, id: &str, worktree: Option<&str>) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        task.worktree = worktree.map(str::to_owned);
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Move a card to the project it is actually about.
    ///
    /// The commander asked for this and the app could not do it: a card filed
    /// against the wrong project could only be discarded and written again,
    /// which destroys the review it carries. Everything bound to the old
    /// project goes with it — whoever held it works there, and the branch and
    /// worktree exist there and nowhere else — so it arrives in the backlog for
    /// somebody in the new project to pick up.
    pub fn take_to(&self, id: &str, repository_id: &str, at: u64) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        if task.repository_id == repository_id {
            bail!("{id} is already filed against {repository_id}");
        }

        let from = std::mem::replace(&mut task.repository_id, repository_id.to_owned());
        task.assignee = None;
        task.worktree = None;
        task.branch = None;
        task.column = Column::Backlog;
        task.evidence.push(Entry::new(
            Evidence::Note {
                text: format!("moved from {from} to {repository_id}"),
            },
            "the board",
            at,
        ));

        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Put a card in a column, in a particular place among the cards there.
    ///
    /// `before` names the card it should sit above; nothing means the bottom.
    /// Dropping between two cards used to be impossible — a card could only
    /// change column, and arrived wherever the list happened to put it.
    pub fn place(&self, id: &str, column: Column, before: Option<&str>) -> Result<Task> {
        let mut state = self.state.lock();

        if !state.tasks.contains_key(id) {
            bail!("unknown task: {id}");
        }

        let mut order: Vec<(String, f64)> = state
            .tasks
            .values()
            .filter(|held| held.column == column && held.id != id)
            .map(|held| (held.id.clone(), held.position))
            .collect();
        order.sort_by(|one, other| one.1.total_cmp(&other.1));

        let seat = before
            .and_then(|wanted| order.iter().position(|(held, _)| held == wanted))
            .unwrap_or(order.len());

        let above = seat.checked_sub(1).and_then(|n| order.get(n)).map(|held| held.1);
        let below = order.get(seat).map(|held| held.1);

        // Renumber the column when the gap has been split past what a float can
        // hold, so the drop lands where it was aimed rather than on top of a
        // neighbour.
        if too_close(above, below) {
            for (n, (held, _)) in order.iter().enumerate() {
                if let Some(task) = state.tasks.get_mut(held) {
                    task.position = n as f64;
                }
            }

            let above = seat.checked_sub(1).map(|n| n as f64);
            let below = if seat < order.len() { Some(seat as f64) } else { None };
            let wanted = placed_between(above, below);

            let task = state.tasks.get_mut(id).expect("checked above");
            task.column = column;
            task.position = wanted;
        } else {
            let wanted = placed_between(above, below);
            let task = state.tasks.get_mut(id).expect("checked above");
            task.column = column;
            task.position = wanted;
        }

        let moved = state.tasks.get(id).expect("checked above").clone();
        self.persist(&state);
        Ok(moved)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock();
        state
            .tasks
            .remove(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;
        let _ = fs::remove_dir_all(self.folder_of(id));
        self.persist(&state);
        Ok(())
    }

    /// Throw away a card that never became anything.
    ///
    /// The crew clears its own board — leftovers from a routine that ran while
    /// nobody was watching are noise, and a commander that cannot remove them
    /// starts mislabelling them "done" instead, which is worse than clutter. But
    /// a card carrying evidence is a record of work that happened, and the crew
    /// does not get to delete those: the human does, in the app.
    pub fn discard(&self, id: &str) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        let records = task
            .evidence
            .iter()
            .filter(|entry| entry.what.is_a_record())
            .count();

        if records > 0 {
            bail!(
                "{id} carries {records} record(s) of work — only a person can remove that"
            );
        }

        let discarded = task.clone();
        state.tasks.remove(id);
        let _ = fs::remove_dir_all(self.folder_of(id));
        self.persist(&state);
        Ok(discarded)
    }
}

/// The position for a card dropped between two others.
///
/// Nothing above it means the top of the column, nothing below means the
/// bottom; between two, the midpoint. Fractions run out eventually — after
/// about fifty splits in the same gap — and the board renumbers that column
/// rather than letting two cards claim one place.
pub fn placed_between(above: Option<f64>, below: Option<f64>) -> f64 {
    match (above, below) {
        (None, None) => 0.0,
        (None, Some(below)) => below - 1.0,
        (Some(above), None) => above + 1.0,
        (Some(above), Some(below)) => (above + below) / 2.0,
    }
}

/// Whether a gap has been split so often the numbers no longer separate.
pub fn too_close(above: Option<f64>, below: Option<f64>) -> bool {
    match (above, below) {
        (Some(above), Some(below)) => (below - above).abs() < f64::EPSILON * 8.0,
        _ => false,
    }
}

/// Where a card goes when the step working on it is over.
///
/// Only forward, and only from `working`: a card somebody moved by hand, or one
/// already waiting on a reviewer, is left where they put it. Nothing written
/// means nothing to look at, so it stays as it is and the commander decides.
pub fn where_a_settled_card_goes(column: Column, files_written: usize) -> Option<Column> {
    if column == Column::Working && files_written > 0 {
        Some(Column::Review)
    } else {
        None
    }
}

/// Where a card goes when somebody who does not merge has decided it is over.
///
/// The commander marking a step done, or finishing a card it held itself,
/// is a decision and not a diff: with or without files written there is
/// nobody left to wait for, and leaving the card in `working` says the
/// opposite. It goes to review rather than done because done is still a merge,
/// or a person saying so — and only forward, never over a column a person chose.
pub fn where_a_decided_card_goes(column: Column) -> Option<Column> {
    matches!(column, Column::Assigned | Column::Working).then_some(Column::Review)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_board_written_before_anyone_signed_their_work_still_opens() {
        let bare: Vec<Entry> =
            serde_json::from_str(r#"[{"kind":"note","text":"taken back from ada"}]"#)
                .expect("the old shape is still read");

        assert_eq!(bare.len(), 1);
        assert_eq!(bare[0].by, "someone", "nobody is invented as the author");
        assert_eq!(bare[0].at, 0);

        let signed: Vec<Entry> = serde_json::from_str(
            r#"[{"what":{"kind":"commit","sha":"abc","subject":"feat: it"},"by":"ada","at":42}]"#,
        )
        .expect("the new shape too");

        assert_eq!(signed[0].by, "ada");
        assert_eq!(signed[0].at, 42);
    }

    #[test]
    fn a_remark_is_not_a_record_of_work() {
        let routing = Evidence::Note {
            text: "X: Nova is the free agent with the closest role".to_owned(),
        };
        assert!(!routing.is_a_record(), "a routing note records nothing");

        for held in [
            Evidence::Commit { sha: "abc".into(), subject: "feat: it".into() },
            Evidence::Diff { files: 1, insertions: 2, deletions: 0 },
            Evidence::PullRequest { url: "https://example.com/1".into() },
            Evidence::Finished { summary: "done".into(), files: 1, insertions: 2, deletions: 0 },
        ] {
            assert!(held.is_a_record(), "{held:?} is work somebody did");
        }
    }

    #[test]
    fn a_card_carrying_only_remarks_can_still_be_discarded() {
        let board = board("remarks");
        let card = board
            .create(CreateTask {
                title: "a duplicate".into(),
                body: String::new(),
                repository_id: "svc-demo".into(),
                worktree: None,
            })
            .expect("create");

        board
            .attach(
                &card.id,
                Evidence::Note { text: "X: Nova is free".into() },
                "the dispatcher",
                7,
            )
            .expect("a routing note");

        board.discard(&card.id).expect("a remark is not a record");
    }

    #[test]
    fn a_card_that_records_work_is_a_persons_to_remove() {
        let board = board("records");
        let card = board
            .create(CreateTask {
                title: "real work".into(),
                body: String::new(),
                repository_id: "svc-demo".into(),
                worktree: None,
            })
            .expect("create");

        board
            .attach(
                &card.id,
                Evidence::Finished { summary: "it works".into(), files: 2, insertions: 9, deletions: 1 },
                "ada",
                7,
            )
            .expect("a finish report");

        let refused = board.discard(&card.id).expect_err("only a person removes that");
        assert!(refused.to_string().contains("1 record"), "{refused}");
    }

    fn board(name: &str) -> Board {
        let dir = std::env::temp_dir().join(format!("agentland-board-{name}"));
        let _ = fs::remove_dir_all(&dir);
        Board::new(dir)
    }

    #[test]
    fn a_card_can_be_reworded_without_losing_its_history() {
        let board = board("edit");
        let card = a_card(&board, None);
        board
            .attach(&card.id, Evidence::Note { text: "seen".into() }, "ada", 3)
            .expect("a note");

        let changed = board
            .edit(
                &card.id,
                EditTask {
                    title: Some("document the whole API".into()),
                    body: Some("every endpoint, with an example".into()),
                },
            )
            .expect("edit");

        assert_eq!(changed.title, "document the whole API");
        assert_eq!(changed.body, "every endpoint, with an example");
        assert_eq!(changed.evidence.len(), 1, "the history stays");

        let blank = board.edit(&card.id, EditTask { title: Some("  ".into()), body: None });
        assert!(blank.is_err(), "a card cannot lose its title");
    }

    #[test]
    fn a_file_put_on_a_card_lands_on_disk_and_in_the_brief() {
        let board = board("attach");
        let card = a_card(&board, None);

        let with = board
            .attach_file(&card.id, "../../shot.png", "image/png", b"PNG", None)
            .expect("attach");
        assert_eq!(with.attachments.len(), 1);
        assert_eq!(with.attachments[0].name, "shot.png", "the name is a name, not a path");
        assert!(with.attachments[0].is_image());

        let path = PathBuf::from(&with.attachments[0].path);
        assert!(path.is_absolute());
        assert!(path.starts_with(&board.data_dir), "it lives in Agentland's own folder");
        assert_eq!(fs::read(&path).expect("written"), b"PNG");

        let again = board
            .attach_file(&card.id, "shot.png", "image/png", b"PNG2", None)
            .expect("a second with the same name");
        assert_eq!(again.attachments[1].name, "shot-2.png", "the first is not overwritten");

        let brief = again.brief();
        assert!(brief.starts_with("document the endpoint\n\n"));
        assert!(brief.contains("Attached to this card"));
        assert!(brief.contains(&again.attachments[0].path), "{brief}");
        assert!(brief.contains(&again.attachments[1].path), "{brief}");

        let bare = a_card(&board, None).brief();
        assert!(!bare.contains("Attached"), "nothing attached, nothing said");
    }

    #[test]
    fn a_file_taken_off_a_card_leaves_the_disk_too() {
        let board = board("detach");
        let card = a_card(&board, None);
        let with = board
            .attach_file(&card.id, "log.txt", "text/plain", b"boom", None)
            .expect("attach");
        let path = PathBuf::from(&with.attachments[0].path);

        let without = board.detach_file(&card.id, "log.txt").expect("detach");
        assert!(without.attachments.is_empty());
        assert!(!path.exists());

        assert!(board.detach_file(&card.id, "log.txt").is_err(), "gone is gone");
    }

    #[test]
    fn deleting_a_card_removes_its_files() {
        let board = board("delete-files");
        let card = a_card(&board, None);
        let with = board
            .attach_file(&card.id, "a.png", "image/png", b"x", None)
            .expect("attach");
        let folder = PathBuf::from(&with.attachments[0].path)
            .parent()
            .expect("a folder")
            .to_path_buf();
        assert!(folder.is_dir());

        board.delete(&card.id).expect("delete");
        assert!(!folder.exists(), "the folder went with the card");
    }

    #[test]
    fn nothing_and_too_much_are_both_refused() {
        let board = board("limits");
        let card = a_card(&board, None);
        assert!(board.attach_file(&card.id, "empty", "text/plain", b"", None).is_err());
        assert!(board.attach_file("t999", "a", "text/plain", b"x", None).is_err());
    }

    #[test]
    fn marks_on_a_picture_are_read_back_in_words_and_the_marked_copy_named() {
        let board = board("marks");
        let card = a_card(&board, None);
        board
            .attach_file(&card.id, "shot.png", "image/png", b"PNG", None)
            .expect("attach");

        let marks = Marks {
            width: 1440,
            height: 900,
            marks: vec![
                Mark { kind: "box".into(), points: vec![[120.4, 40.0], [340.0, 90.6]], text: "overlaps the menu".into() },
                Mark { kind: "arrow".into(), points: vec![[10.0, 10.0], [200.0, 300.0]], text: String::new() },
                Mark { kind: "pen".into(), points: vec![[5.0, 9.0], [50.0, 2.0], [30.0, 40.0]], text: "wobbly".into() },
                Mark { kind: "pin".into(), points: vec![[700.0, 20.0]], text: "here".into() },
            ],
        };
        let with = board.set_marks(&card.id, "shot.png", marks).expect("marks");
        assert_eq!(with.attachments[0].marks.as_ref().map(|held| held.marks.len()), Some(4));

        let legend = with.attachments[0].marks.as_ref().unwrap().legend();
        assert_eq!(legend[0], "1. box from (120, 40) to (340, 91): \"overlaps the menu\"");
        assert_eq!(legend[1], "2. arrow from (10, 10) pointing at (200, 300)");
        assert_eq!(legend[2], "3. freehand stroke within (5, 2) to (50, 40): \"wobbly\"");
        assert_eq!(legend[3], "4. pin at (700, 20): \"here\"");

        let brief = with.brief();
        assert!(brief.contains("1440×900"), "{brief}");
        assert!(brief.contains("1. box from (120, 40)"), "{brief}");
        assert!(brief.contains("Address each one"), "{brief}");
        assert!(!brief.contains("marked copy"), "there is no copy yet");

        let copied = board
            .attach_file(&card.id, "shot.marked.png", "image/png", b"PNG+marks", Some("shot.png"))
            .expect("the marked copy");
        assert_eq!(copied.attachments.len(), 2);
        assert_eq!(copied.attachments[1].derived_from.as_deref(), Some("shot.png"));

        let again = board
            .attach_file(&card.id, "shot.marked.png", "image/png", b"PNG+more", Some("shot.png"))
            .expect("the copy again");
        assert_eq!(again.attachments.len(), 2, "the copy is replaced, not numbered");
        assert_eq!(fs::read(&again.attachments[1].path).unwrap(), b"PNG+more");

        let brief = again.brief();
        assert!(brief.contains("Read the marked copy"), "{brief}");
        assert!(brief.contains(&again.attachments[1].path), "{brief}");
        let listed = brief.matches("\n- ").count();
        assert_eq!(listed, 1, "the copy is not listed as a file of its own: {brief}");

        assert!(
            board.attach_file(&card.id, "x.png", "image/png", b"x", Some("nothing.png")).is_err(),
            "a copy of nothing is refused"
        );

        let cleared = board
            .set_marks(&card.id, "shot.png", Marks { width: 1, height: 1, marks: vec![] })
            .expect("clear");
        assert!(cleared.attachments[0].marks.is_none());

        let copy_path = PathBuf::from(&again.attachments[1].path);
        let without = board.detach_file(&card.id, "shot.png").expect("detach the original");
        assert!(without.attachments.is_empty(), "the copy went with it");
        assert!(!copy_path.exists());
    }

    #[test]
    fn marks_go_on_pictures_only() {
        let board = board("marks-text");
        let card = a_card(&board, None);
        board
            .attach_file(&card.id, "log.txt", "text/plain", b"boom", None)
            .expect("attach");
        assert!(board
            .set_marks(&card.id, "log.txt", Marks { width: 1, height: 1, marks: vec![Mark { kind: "pin".into(), points: vec![[1.0, 1.0]], text: String::new() }] })
            .is_err());
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(safe_name("/etc/passwd"), "passwd");
        assert_eq!(safe_name("..\\..\\win.ini"), "win.ini");
        assert_eq!(safe_name("...hidden"), "hidden");
        assert_eq!(safe_name("a:b*c?.png"), "a_b_c_.png");
        assert_eq!(safe_name(""), "file");
        assert_eq!(safe_name("Screen Shot 2026-08-13 at 17.20.45.png"), "Screen Shot 2026-08-13 at 17.20.45.png");
    }

    fn a_card(board: &Board, worktree: Option<&str>) -> Task {
        board
            .create(CreateTask {
                title: "document the endpoint".to_owned(),
                body: String::new(),
                repository_id: "demo".to_owned(),
                worktree: worktree.map(str::to_owned),
            })
            .unwrap()
    }

    #[test]
    fn a_card_someone_is_holding_can_be_found_again_after_a_restart() {
        let board = board("still-holding");
        let mine = a_card(&board, Some("ada-tree"));
        let theirs = a_card(&board, Some("ada-tree"));
        board.record_assignment(&mine.id, "ada", "ada-tree", "agent/ada-tree").unwrap();
        board.record_assignment(&theirs.id, "zen", "ada-tree", "agent/ada-tree").unwrap();
        board.move_to(&theirs.id, Column::Done).unwrap();

        let ada_is_holding: Vec<_> = board
            .list()
            .into_iter()
            .filter(|task| task.assignee.as_deref() == Some("ada"))
            .filter(|task| matches!(task.column, Column::Working))
            .collect();

        assert_eq!(ada_is_holding.len(), 1, "the one still in flight, not the finished one");
        assert_eq!(ada_is_holding[0].id, mine.id);
    }

    #[test]
    fn a_card_filed_against_the_wrong_project_can_be_taken_to_the_right_one() {
        let board = board("take-to");
        let card = a_card(&board, None);

        board
            .record_assignment(&card.id, "ada", "svc-demo-tree", "agent/svc-demo-tree")
            .expect("somebody picked it up");
        board
            .attach(
                &card.id,
                Evidence::Reviewed {
                    verdict: "approve".into(),
                    summary: "reads right".into(),
                },
                "rex",
                50,
            )
            .expect("a review");

        let moved = board.take_to(&card.id, "agentland", 100).expect("it moves");

        assert_eq!(moved.repository_id, "agentland");
        assert_eq!(moved.assignee, None, "whoever held it works in the old project");
        assert_eq!(moved.worktree, None, "that worktree is in the old project only");
        assert_eq!(moved.branch, None);
        assert_eq!(moved.column, Column::Backlog);
        assert!(
            moved.evidence.iter().any(|entry| matches!(entry.what, Evidence::Reviewed { .. })),
            "moving is the point: discarding and writing it again would lose the review"
        );
        assert!(
            moved.evidence.iter().any(|entry| matches!(&entry.what, Evidence::Note { text } if text.contains("moved from"))),
            "and the move itself reads on the card"
        );
    }

    #[test]
    fn taking_a_card_where_it_already_is_is_refused_rather_than_recorded() {
        let board = board("take-to-same");
        let card = a_card(&board, None);
        let home = card.repository_id.clone();

        assert!(board.take_to(&card.id, &home, 100).is_err());
    }

    /// The supervisor asks a card whether work has been recorded on it before
    /// it calls a step finished. Handing the card out writes a note, and for a
    /// while any evidence counted: three cards were called settled 35 to 42
    /// seconds before the commit that did the work, and a review card one
    /// second after it was handed over.
    #[test]
    fn handing_a_card_out_records_no_work_on_it() {
        let board = board("assignment-is-not-work");
        let card = a_card(&board, None);

        board
            .record_assignment(&card.id, "ada", "ada-tree", "agent/ada-tree")
            .expect("handed out");
        board
            .attach(
                &card.id,
                Evidence::Note {
                    text: "X: Ada is the free agent with the closest role".into(),
                },
                "the dispatcher",
                50,
            )
            .expect("the routing note the dispatcher writes");

        let held = board.get(&card.id).expect("the card");
        assert!(
            !held.evidence.is_empty(),
            "the note is still written, so the history reads"
        );
        assert!(
            !held.evidence.iter().any(|entry| entry.what.is_a_record()),
            "but nothing has been done on it yet"
        );

        board
            .attach(
                &card.id,
                Evidence::Commit {
                    sha: "ecc2e32".into(),
                    subject: "serve /metrics".into(),
                },
                "ada",
                100,
            )
            .expect("work lands");

        let done = board.get(&card.id).expect("the card");
        assert!(
            done.evidence.iter().any(|entry| entry.what.is_a_record()),
            "and once it has, the card says so"
        );
    }

    #[test]
    fn a_step_that_wrote_something_leaves_its_card_waiting_to_be_read() {
        assert_eq!(
            where_a_settled_card_goes(Column::Working, 3),
            Some(Column::Review),
            "the work is written and nobody has looked at it"
        );
    }

    #[test]
    fn a_step_that_wrote_nothing_moves_no_card() {
        assert_eq!(where_a_settled_card_goes(Column::Working, 0), None);
    }

    #[test]
    fn a_decision_moves_the_card_on_whether_or_not_anything_was_written() {
        assert_eq!(where_a_decided_card_goes(Column::Working), Some(Column::Review));
        assert_eq!(where_a_decided_card_goes(Column::Assigned), Some(Column::Review));
    }

    #[test]
    fn a_decision_never_moves_a_card_backwards_or_past_a_person() {
        for held in [Column::Review, Column::Ready, Column::Done, Column::Backlog] {
            assert_eq!(where_a_decided_card_goes(held), None, "{held:?} is not the app's to change");
        }
    }

    #[test]
    fn a_card_somebody_already_moved_is_left_where_they_put_it() {
        for held in [Column::Review, Column::Ready, Column::Done, Column::Backlog] {
            assert_eq!(where_a_settled_card_goes(held, 5), None, "{held:?} is not the app's to change");
        }
    }

    #[test]
    fn a_card_dropped_between_two_others_lands_between_them() {
        let board = board("place-between");
        let first = a_card(&board, None);
        let second = a_card(&board, None);
        let third = a_card(&board, None);

        let moved = board
            .place(&third.id, Column::Backlog, Some(&second.id))
            .expect("it moves");

        let mut held: Vec<(String, f64)> = board
            .list()
            .into_iter()
            .filter(|task| task.column == Column::Backlog)
            .map(|task| (task.id, task.position))
            .collect();
        held.sort_by(|one, other| one.1.total_cmp(&other.1));

        assert_eq!(
            held.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            vec![first.id.as_str(), moved.id.as_str(), second.id.as_str()],
            "it sits above the card it was dropped on"
        );
    }

    #[test]
    fn a_card_dropped_on_nothing_goes_to_the_bottom() {
        let board = board("place-bottom");
        let first = a_card(&board, None);
        let second = a_card(&board, None);

        board.place(&first.id, Column::Backlog, None).expect("it moves");

        let mut held: Vec<(String, f64)> = board
            .list()
            .into_iter()
            .map(|task| (task.id, task.position))
            .collect();
        held.sort_by(|one, other| one.1.total_cmp(&other.1));

        assert_eq!(held.last().map(|(id, _)| id.as_str()), Some(first.id.as_str()));
        assert_eq!(held.first().map(|(id, _)| id.as_str()), Some(second.id.as_str()));
    }

    #[test]
    fn a_card_arriving_in_a_column_lands_at_the_bottom() {
        let board = board("move-bottom");
        let first = a_card(&board, None);
        let second = a_card(&board, None);
        let third = a_card(&board, None);

        board.place(&third.id, Column::Done, None).expect("first into done");
        board.place(&first.id, Column::Done, None).expect("second into done");
        let last = board.move_to(&second.id, Column::Done).expect("third into done");

        let positions = |id: &str| board.get(id).expect("held").position;
        assert!(
            positions(&third.id) < positions(&first.id) && positions(&first.id) < last.position,
            "done reads in the order cards arrived, not the order they sat in backlog"
        );
    }

    #[test]
    fn a_card_already_in_the_column_keeps_its_place() {
        let board = board("move-stays");
        let above = a_card(&board, None);
        let card = a_card(&board, None);
        board.place(&card.id, Column::Backlog, Some(&above.id)).expect("put above");
        let before = board.get(&card.id).expect("held").position;

        let moved = board.move_to(&card.id, Column::Backlog).expect("same column");

        assert_eq!(moved.position, before, "moving to where it already is changes nothing");
    }

    #[test]
    fn placing_moves_the_column_too() {
        let board = board("place-column");
        let card = a_card(&board, None);

        let moved = board.place(&card.id, Column::Review, None).expect("it moves");

        assert_eq!(moved.column, Column::Review);
    }

    #[test]
    fn the_midpoint_is_taken_and_the_ends_step_outward() {
        assert_eq!(placed_between(Some(1.0), Some(2.0)), 1.5);
        assert_eq!(placed_between(None, Some(1.0)), 0.0);
        assert_eq!(placed_between(Some(4.0), None), 5.0);
        assert_eq!(placed_between(None, None), 0.0);
    }

    #[test]
    fn a_gap_split_past_what_a_float_holds_is_renumbered() {
        let board = board("place-crowded");
        let first = a_card(&board, None);
        let second = a_card(&board, None);
        let mover = a_card(&board, None);

        // Squeeze the two neighbours together until nothing fits between them.
        {
            let mut state = board.state.lock();
            state.tasks.get_mut(&first.id).unwrap().position = 1.0;
            state.tasks.get_mut(&second.id).unwrap().position = 1.0 + f64::EPSILON;
        }

        assert!(too_close(Some(1.0), Some(1.0 + f64::EPSILON)));

        board
            .place(&mover.id, Column::Backlog, Some(&second.id))
            .expect("it still lands");

        let mut held: Vec<(String, f64)> = board
            .list()
            .into_iter()
            .map(|task| (task.id, task.position))
            .collect();
        held.sort_by(|one, other| one.1.total_cmp(&other.1));

        assert_eq!(
            held.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            vec![first.id.as_str(), mover.id.as_str(), second.id.as_str()],
            "renumbering keeps the order the drop asked for"
        );
    }

    #[test]
    fn a_card_that_became_nothing_can_be_thrown_away() {
        let board = board("discard");
        let card = a_card(&board, None);

        let gone = board.discard(&card.id).unwrap();

        assert_eq!(gone.id, card.id);
        assert!(board.get(&card.id).is_none());
        assert!(board.discard(&card.id).is_err(), "and it is gone for good");
    }

    #[test]
    fn a_card_that_records_work_is_a_record_the_crew_may_not_delete() {
        let board = board("discard-evidence");
        let card = a_card(&board, None);
        board
            .attach(
                &card.id,
                Evidence::Commit { sha: "abc1234".into(), subject: "feat: it".into() },
                "ada",
                7,
            )
            .unwrap();

        let refused = board.discard(&card.id).unwrap_err().to_string();

        assert!(refused.contains("record"), "it says why: {refused}");
        assert!(board.get(&card.id).is_some(), "and the card is still there");
    }

    #[test]
    fn a_card_can_be_born_belonging_to_a_worktree() {
        let board = board("bound");

        let card = a_card(&board, Some("ada-tree"));

        assert_eq!(card.worktree.as_deref(), Some("ada-tree"));
        assert_eq!(card.assignee, None);
    }

    #[test]
    fn taking_a_card_back_frees_it_and_says_who_held_it() {
        let board = board("release");
        let card = a_card(&board, Some("ada-tree"));
        board
            .record_assignment(&card.id, "nova", "ada-tree", "agent/ada-tree")
            .unwrap();

        let freed = board.release(&card.id).unwrap();

        assert_eq!(freed.assignee, None, "nobody holds it now");
        assert_eq!(freed.branch, None, "the branch was the holder's, not the card's");
        assert!(matches!(freed.column, Column::Backlog));
        assert_eq!(
            freed.worktree.as_deref(),
            Some("ada-tree"),
            "where the work belongs is a property of the work",
        );
        assert!(
            freed
                .evidence
                .iter()
                .any(|held| matches!(&held.what, Evidence::Note { text } if text.contains("nova"))),
            "the card remembers who had it",
        );
    }

    #[test]
    fn a_card_nobody_held_can_still_be_taken_back() {
        let board = board("release-free");
        let card = a_card(&board, None);

        let freed = board.release(&card.id).unwrap();

        assert_eq!(freed.assignee, None);
        assert!(freed.evidence.is_empty(), "nothing to say about nobody");
    }

    #[test]
    fn where_a_card_belongs_can_be_decided_after_it_is_written() {
        let board = board("bind");
        let card = a_card(&board, None);

        let bound = board.bind_to_worktree(&card.id, Some("ada-tree")).unwrap();
        assert_eq!(bound.worktree.as_deref(), Some("ada-tree"));

        let loosened = board.bind_to_worktree(&card.id, None).unwrap();
        assert_eq!(loosened.worktree, None);
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}
