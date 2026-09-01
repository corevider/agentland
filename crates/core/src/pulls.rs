use serde::{Deserialize, Serialize};

/// What one check said about a commit.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Check {
    #[serde(default)]
    pub name: String,
    /// A check run's own state: QUEUED, IN_PROGRESS, COMPLETED.
    #[serde(default)]
    pub status: String,
    /// A check run's verdict: SUCCESS, FAILURE, CANCELLED, SKIPPED, NEUTRAL.
    #[serde(default)]
    pub conclusion: String,
    /// A commit status uses one field for both: SUCCESS, PENDING, FAILURE.
    #[serde(default)]
    pub state: String,
}

impl Check {
    pub fn is_finished(&self) -> bool {
        if !self.state.is_empty() {
            return !self.state.eq_ignore_ascii_case("PENDING")
                && !self.state.eq_ignore_ascii_case("EXPECTED");
        }

        self.status.is_empty() || self.status.eq_ignore_ascii_case("COMPLETED")
    }

    /// Whether this check is a reason not to merge.
    ///
    /// Skipped and neutral are not failures, and a check nobody ran is not one
    /// either — treating them as failures sends work back to an agent that has
    /// nothing to fix.
    pub fn is_failing(&self) -> bool {
        let verdict = if self.state.is_empty() {
            &self.conclusion
        } else {
            &self.state
        };

        ["FAILURE", "TIMED_OUT", "STARTUP_FAILURE", "ERROR", "ACTION_REQUIRED"]
            .iter()
            .any(|bad| verdict.eq_ignore_ascii_case(bad))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PullState {
    #[serde(default)]
    pub number: u64,
    #[serde(default)]
    pub url: String,
    /// OPEN, MERGED, CLOSED.
    #[serde(default)]
    pub state: String,
    /// MERGEABLE, CONFLICTING, UNKNOWN.
    #[serde(default)]
    pub mergeable: String,
    #[serde(default, rename = "mergeStateStatus")]
    pub merge_state: String,
    /// The branch this is asking to merge into. A pull request can target any
    /// branch, and the repository's default is not always the one it conflicts
    /// with — computing the conflict against the wrong base finds none.
    #[serde(default, rename = "baseRefName")]
    pub base: String,
    /// APPROVED, CHANGES_REQUESTED, REVIEW_REQUIRED, or empty when the
    /// repository asks for no review at all.
    #[serde(default, rename = "reviewDecision")]
    pub review: String,
    #[serde(default, rename = "statusCheckRollup")]
    pub checks: Vec<Check>,
}

/// Where a card stands once its work is on a pull request.
///
/// The five that need something to happen are separate from the one that does
/// not: a card nobody has reviewed and a card whose tests are red both sit in
/// review, but only one of them is the agent's problem again.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Standing {
    /// Merged upstream. The work is done and the card is finished.
    Merged,
    /// Closed without merging. Somebody decided against it.
    Closed,
    /// Nothing is in the way: it can be merged whenever a person says so.
    Ready,
    /// The branch cannot merge as it is — the agent has to rebase or resolve.
    Conflicted,
    /// The base has moved on and the branch has not caught up. Nothing to
    /// resolve, only something to update — a different job and a different
    /// sentence, and telling somebody to resolve conflicts they do not have
    /// wastes a turn while they go looking.
    Behind,
    /// A check said no. The agent that wrote it has something to fix.
    ChecksFailing { failed: Vec<String> },
    /// It is somebody else's turn: a review, or a check still running.
    Waiting { why: String },
}

impl Standing {
    /// Whether this is the agent's problem again rather than a person's.
    pub fn goes_back_to_the_agent(&self) -> bool {
        matches!(
            self,
            Standing::Conflicted | Standing::Behind | Standing::ChecksFailing { .. }
        )
    }
}

/// How long a pull request has to have been watched before an empty check list
/// means "this repository runs no checks" rather than "they have not started".
///
/// Measured on a real one: the card reached `ready` twelve seconds after the
/// pull request was opened, while the workflow was still being registered.
/// GitHub said CLEAN and reported no checks, because at that moment there were
/// none — and for a minute the board offered a merge button on work whose tests
/// had not run.
const CHECKS_GET_A_MOMENT: u64 = 120;

/// Read a pull request's state as one word about what happens next.
///
/// `seen_for` is how long this pull request has been watched, in seconds. It is
/// the difference between a repository with no CI and a repository whose CI has
/// not woken up yet, which nothing the forge reports can tell apart.
pub fn where_it_stands(pull: &PullState, seen_for: u64) -> Standing {
    if pull.state.eq_ignore_ascii_case("MERGED") {
        return Standing::Merged;
    }

    if pull.state.eq_ignore_ascii_case("CLOSED") {
        return Standing::Closed;
    }

    // A conflict outranks a red check: rebasing changes what the checks even
    // ran against, so telling an agent to fix a test first is telling it to fix
    // a result that is about to be replaced.
    if pull.mergeable.eq_ignore_ascii_case("CONFLICTING")
        || pull.merge_state.eq_ignore_ascii_case("DIRTY")
    {
        return Standing::Conflicted;
    }

    // Being behind comes before the checks for the same reason a conflict does:
    // catching up re-runs them against something else.
    if pull.merge_state.eq_ignore_ascii_case("BEHIND") {
        return Standing::Behind;
    }

    let failed: Vec<String> = pull
        .checks
        .iter()
        .filter(|check| check.is_failing())
        .map(|check| {
            if check.name.is_empty() {
                "a check".to_owned()
            } else {
                check.name.clone()
            }
        })
        .collect();

    if !failed.is_empty() {
        return Standing::ChecksFailing { failed };
    }

    let running = pull.checks.iter().filter(|check| !check.is_finished()).count();
    if running > 0 {
        return Standing::Waiting {
            why: format!(
                "{running} check{} still running",
                if running == 1 { " is" } else { "s are" }
            ),
        };
    }

    if pull.review.eq_ignore_ascii_case("CHANGES_REQUESTED") {
        return Standing::Waiting {
            why: "a reviewer asked for changes".to_owned(),
        };
    }

    if pull.review.eq_ignore_ascii_case("REVIEW_REQUIRED") {
        return Standing::Waiting {
            why: "nobody has reviewed it yet".to_owned(),
        };
    }

    // `UNKNOWN` is what GitHub says while it is still working the merge out,
    // and calling that ready would offer a merge button that fails.
    if pull.mergeable.eq_ignore_ascii_case("UNKNOWN") {
        return Standing::Waiting {
            why: "GitHub has not worked out whether it merges yet".to_owned(),
        };
    }

    if pull.checks.is_empty() && seen_for < CHECKS_GET_A_MOMENT {
        return Standing::Waiting {
            why: "no checks have reported yet".to_owned(),
        };
    }

    // Nothing above named a reason, so the last word is the forge's own. CLEAN
    // is the only one that means nothing is in the way; BLOCKED, UNSTABLE and
    // BEHIND each mean something is, whether or not this can say what.
    let clean = pull.merge_state.is_empty() || pull.merge_state.eq_ignore_ascii_case("CLEAN");
    if !clean {
        return Standing::Waiting {
            why: format!("the forge says {}", pull.merge_state.to_lowercase()),
        };
    }

    Standing::Ready
}

/// Lines that are the failure itself.
const THE_FAILURE_ITSELF: &[&str] = &[
    "traceback", "assertionerror", "panicked at", "exception:", "error:",
    "fatal:", "--- fail", "not ok", "✗",
];

/// Lines that are somebody counting the failures up afterwards.
const A_TALLY_OF_IT: &[&str] = &["failed", "failure", "exit code", "##[error]", "error"];

/// Lines kept after the anchor, where most tools print the detail.
const AFTER: usize = 4;

/// Lines kept before it, when the anchor is the failure itself.
const A_TRACE: usize = 16;

fn says(line: &str, words: &[&str]) -> bool {
    let lower = line.to_lowercase();
    words.iter().any(|word| lower.contains(word))
}

/// A CI log's own prefix: `job<tab>step<tab>2026-09-01T07:59:16.1234567Z `.
fn without_the_scaffolding(line: &str) -> &str {
    let after_tabs = line.rsplit('\t').next().unwrap_or(line);
    let trimmed = after_tabs.trim_start();

    // A leading ISO timestamp is the runner's, not the program's.
    match trimmed.split_once(' ') {
        Some((first, rest))
            if first.len() >= 20 && first.starts_with("20") && first.ends_with('Z') =>
        {
            rest
        }
        _ => trimmed,
    }
}

/// The part of a CI log worth putting in front of whoever has to fix it.
///
/// A run's log is tens of thousands of lines of setup, and an agent handed all
/// of it reads none of it. What is wanted is the failure and the lines around
/// it, so the excerpt is taken from the last thing that looks like trouble —
/// and from the tail when nothing does, because a log that ends without saying
/// why is still better read at the end than at the beginning.
pub fn failure_excerpt(log: &str, budget: usize) -> String {
    let lines: Vec<&str> = log
        .lines()
        .map(without_the_scaffolding)
        .filter(|line| !line.trim().is_empty())
        .collect();

    if lines.is_empty() {
        return String::new();
    }

    // The failure itself outranks the tally at the bottom. A suite that keeps
    // going after one test fails ends with six hundred lines of passing output
    // and `SOME TESTS FAILED` on the last line — measured, and the excerpt came
    // back full of tests that passed. The tally is only the anchor when nothing
    // in the log says what actually broke.
    let anchor = lines
        .iter()
        .rposition(|line| says(line, THE_FAILURE_ITSELF))
        .or_else(|| lines.iter().rposition(|line| says(line, A_TALLY_OF_IT)));

    // A few lines after the failure carry its detail; the ones before carry
    // what was being done when it happened.
    let end = anchor
        .map(|at| (at + AFTER).min(lines.len()))
        .unwrap_or(lines.len());

    // How far back to walk. A stack trace is a dozen lines; filling the whole
    // budget backwards from it drags in whatever the suite happened to print
    // before, which on a real run was ten lines of tests that passed. When the
    // only anchor is a tally at the bottom there is no trace to bound, so the
    // budget is the bound.
    let reach = if anchor.map(|at| says(lines[at], THE_FAILURE_ITSELF)).unwrap_or(false) {
        A_TRACE.min(end)
    } else {
        end
    };

    let mut taken: Vec<&str> = Vec::new();
    let mut size = 0;

    for line in lines[end - reach..end].iter().rev() {
        let cost = line.chars().count() + 1;
        if size + cost > budget && !taken.is_empty() {
            break;
        }
        size += cost;
        taken.push(line);
    }

    taken.reverse();
    taken.join("\n")
}

/// The files `git merge-tree --write-tree --name-only` says would conflict.
///
/// Its first line is the tree it wrote, then one path per conflicting file,
/// then a blank line and the messages it printed while merging. Only the middle
/// is wanted, and reading the messages as paths would report
/// `CONFLICT (content): Merge conflict in a.txt` as a filename.
pub fn merge_tree_conflicts(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// What to tell the agent whose branch will not merge.
///
/// It names the files rather than the problem. "The branch conflicts" sends
/// somebody to go and find out what; a list sends them to the first line they
/// have to change.
pub fn conflict_brief(number: u64, base: &str, branch: &str, files: &[String]) -> String {
    let named = match files.len() {
        0 => "Git has not said which files yet".to_owned(),
        1 => format!("One file conflicts: {}", files[0]),
        count => format!("{count} files conflict: {}", files.join(", ")),
    };

    format!(
        "Pull request #{number} cannot merge: {branch} conflicts with {base}. {named}.          Rebase {branch} onto origin/{base}, resolve them, run the tests and push.          The card is back in working."
    )
}

/// What to tell the agent whose branch is only out of date.
pub fn behind_brief(number: u64, base: &str, branch: &str) -> String {
    format!(
        "Pull request #{number} is behind {base}. Nothing conflicts — update {branch} from          origin/{base}, push, and the checks run again. The card is back in working."
    )
}

/// A verdict an agent can reach on somebody else's work.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Approved,
    ChangesRequested,
    Commented,
}

impl Verdict {
    pub fn read(word: &str) -> Option<Verdict> {
        match word.trim().to_lowercase().replace(['-', ' '], "_").as_str() {
            "approved" | "approve" => Some(Verdict::Approved),
            "changes_requested" | "request_changes" | "changes" => Some(Verdict::ChangesRequested),
            "commented" | "comment" => Some(Verdict::Commented),
            _ => None,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Verdict::Approved => "approved",
            Verdict::ChangesRequested => "requested changes",
            Verdict::Commented => "commented",
        }
    }

    /// Whether this hands the work back to whoever wrote it.
    pub fn sends_it_back(self) -> bool {
        matches!(self, Verdict::ChangesRequested)
    }
}

/// Whether this agent may pass judgement on this card.
///
/// Nobody reviews their own work. It is the one rule a review has: an agent
/// that can approve what it just wrote is not a reviewer, it is a rubber stamp
/// with a salary.
pub fn may_review(reviewer: &str, assignee: Option<&str>) -> Result<(), String> {
    if reviewer.trim().is_empty() {
        return Err("a review has to be signed".to_owned());
    }

    match assignee {
        Some(who) if who == reviewer => Err(format!(
            "{reviewer} wrote this one — a review is somebody else's job"
        )),
        _ => Ok(()),
    }
}

/// What a review says on the pull request, in the reviewer's name.
///
/// Every agent here pushes as the same GitHub account, and GitHub will not let
/// an account approve its own pull request — so the verdict lives on the card
/// and this is what goes to the forge: a comment that says who reached it and
/// what they decided, where the people looking at the pull request will see it.
pub fn review_comment(reviewer: &str, verdict: Verdict, summary: &str) -> String {
    let said = summary.trim();
    let body = if said.is_empty() {
        String::new()
    } else {
        format!("\n\n{said}")
    };

    format!("**{reviewer}** {} this.{body}", verdict.word())
}

/// What to say on the card about where it stands.
pub fn in_a_line(standing: &Standing) -> String {
    match standing {
        Standing::Merged => "merged".to_owned(),
        Standing::Closed => "closed without merging".to_owned(),
        Standing::Ready => "ready to merge".to_owned(),
        Standing::Conflicted => "the branch conflicts with the base".to_owned(),
        Standing::Behind => "the base has moved on and the branch has not".to_owned(),
        Standing::ChecksFailing { failed } => format!("checks failing: {}", failed.join(", ")),
        Standing::Waiting { why } => why.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Watched long enough that an empty check list means what it says.
    const LONG_ENOUGH: u64 = CHECKS_GET_A_MOMENT + 1;

    fn open(mergeable: &str, review: &str, checks: Vec<Check>) -> PullState {
        PullState {
            number: 12,
            url: "https://github.com/o/r/pull/12".to_owned(),
            state: "OPEN".to_owned(),
            mergeable: mergeable.to_owned(),
            merge_state: "CLEAN".to_owned(),
            base: "main".to_owned(),
            review: review.to_owned(),
            checks,
        }
    }

    fn check(name: &str, status: &str, conclusion: &str) -> Check {
        Check {
            name: name.to_owned(),
            status: status.to_owned(),
            conclusion: conclusion.to_owned(),
            state: String::new(),
        }
    }

    #[test]
    fn an_excerpt_keeps_the_failure_and_drops_the_setup() {
        let log = concat!(
            "test\tRun tests\t2026-09-01T07:59:16.1234567Z Setting up python\n",
            "test\tRun tests\t2026-09-01T07:59:17.1234567Z Collected 41 items\n",
            "test\tRun tests\t2026-09-01T07:59:18.1234567Z test_queue.py::test_next FAILED\n",
            "test\tRun tests\t2026-09-01T07:59:19.1234567Z E   AssertionError: expected 3, got 0\n",
            "test\tRun tests\t2026-09-01T07:59:20.1234567Z 1 failed, 40 passed\n",
        );

        let held = failure_excerpt(log, 4000);

        assert!(held.contains("AssertionError: expected 3, got 0"), "{held}");
        assert!(held.contains("1 failed, 40 passed"), "{held}");
        assert!(!held.contains("2026-09-01T"), "the runner's timestamps are not the failure");
        assert!(!held.contains('\t'), "nor is its job and step: {held}");
    }

    #[test]
    fn a_suite_that_keeps_going_after_a_failure_still_points_at_the_failure() {
        // The shape of a real run: the traceback near the top, six hundred
        // lines of passing tests after it, and the tally on the last line.
        let mut log = String::from(
            "test\tRun the test suite\t2026-09-01T08:22:58.1083727Z Traceback (most recent call last):\n\
             test\tRun the test suite\t2026-09-01T08:22:58.1096546Z AssertionError: expected 3, got 0\n",
        );
        for i in 0..600 {
            log.push_str(&format!(
                "test\tRun the test suite\t2026-09-01T08:23:05.7524686Z   OK   something {i} passed\n"
            ));
        }
        log.push_str("test\tRun the test suite\t2026-09-01T08:23:05.7641748Z SOME TESTS FAILED\n");
        log.push_str("test\tRun the test suite\t2026-09-01T08:23:05.7659319Z ##[error]Process completed with exit code 1.\n");

        let held = failure_excerpt(&log, 1800);

        assert!(held.starts_with("Traceback"), "it opens on the failure: {held}");
        assert!(held.contains("AssertionError: expected 3, got 0"), "{held}");

        // A few lines after the failure are kept on purpose — most tools print
        // the detail there. Six hundred are not.
        let passing = held.lines().filter(|line| line.contains("OK   something")).count();
        assert!(passing <= 3, "{passing} lines of passing output came along");
    }

    #[test]
    fn a_tally_is_the_anchor_only_when_nothing_says_what_broke() {
        let log = "building\nlinking\nSOME TESTS FAILED\n";

        let held = failure_excerpt(log, 1800);

        assert!(held.contains("SOME TESTS FAILED"), "{held}");
    }

    #[test]
    fn an_excerpt_stays_within_its_budget_and_keeps_the_end() {
        let mut log = String::new();
        for i in 0..4000 {
            log.push_str(&format!("noise line {i}\n"));
        }
        log.push_str("Error: the thing broke\n");

        let held = failure_excerpt(&log, 300);

        assert!(held.len() <= 400, "it is {} characters", held.len());
        assert!(held.contains("Error: the thing broke"), "the failure survives the trim");
    }

    #[test]
    fn a_log_that_never_says_why_is_read_from_the_end() {
        let log = "step one\nstep two\nstep three\n";

        let held = failure_excerpt(log, 4000);

        assert!(held.ends_with("step three"), "{held}");
    }

    #[test]
    fn an_empty_log_says_nothing_rather_than_something() {
        assert_eq!(failure_excerpt("", 4000), "");
        assert_eq!(failure_excerpt("   \n\n  \n", 4000), "");
    }

    #[test]
    fn a_green_approved_pull_is_ready() {
        let pull = open("MERGEABLE", "APPROVED", vec![check("test", "COMPLETED", "SUCCESS")]);

        assert_eq!(where_it_stands(&pull, LONG_ENOUGH), Standing::Ready);
    }

    #[test]
    fn a_repository_that_asks_for_no_review_does_not_wait_for_one() {
        let pull = open("MERGEABLE", "", vec![check("test", "COMPLETED", "SUCCESS")]);

        assert_eq!(where_it_stands(&pull, LONG_ENOUGH), Standing::Ready);
    }

    #[test]
    fn a_merged_pull_is_finished_however_red_it_was() {
        let mut pull = open("CONFLICTING", "CHANGES_REQUESTED", vec![check("test", "COMPLETED", "FAILURE")]);
        pull.state = "MERGED".to_owned();

        assert_eq!(where_it_stands(&pull, LONG_ENOUGH), Standing::Merged);
    }

    #[test]
    fn the_files_that_conflict_are_read_and_the_chatter_is_not() {
        // Exactly what git 2.53 prints on a real two-file conflict.
        let printed = "94774d0bee5b6c5944249127235e272edb502b47\n                       a.txt\n                       src/b.rs\n                       \n                       Auto-merging a.txt\n                       CONFLICT (content): Merge conflict in a.txt\n";

        assert_eq!(merge_tree_conflicts(printed), vec!["a.txt", "src/b.rs"]);
    }

    #[test]
    fn a_merge_that_would_be_clean_names_no_files() {
        let printed = "94774d0bee5b6c5944249127235e272edb502b47\n";

        assert!(merge_tree_conflicts(printed).is_empty());
        assert!(merge_tree_conflicts("").is_empty());
    }

    #[test]
    fn a_branch_that_is_only_out_of_date_is_told_to_update_not_to_resolve() {
        let mut pull = open("MERGEABLE", "APPROVED", vec![check("test", "COMPLETED", "SUCCESS")]);
        pull.merge_state = "BEHIND".to_owned();

        let standing = where_it_stands(&pull, LONG_ENOUGH);
        assert_eq!(standing, Standing::Behind);
        assert!(standing.goes_back_to_the_agent(), "only the agent can update it");

        let said = behind_brief(7, "main", "agent/x");
        assert!(said.contains("Nothing conflicts"), "{said}");
        assert!(!said.to_lowercase().contains("resolve"), "there is nothing to resolve: {said}");
    }

    #[test]
    fn a_conflict_brief_names_the_files_rather_than_the_problem() {
        let one = conflict_brief(7, "main", "agent/x", &["a.txt".to_owned()]);
        assert!(one.contains("One file conflicts: a.txt"), "{one}");

        let two = conflict_brief(
            7,
            "main",
            "agent/x",
            &["a.txt".to_owned(), "src/b.rs".to_owned()],
        );
        assert!(two.contains("2 files conflict: a.txt, src/b.rs"), "{two}");
        assert!(two.contains("Rebase agent/x onto origin/main"), "{two}");

        // Git not answering is said, rather than an empty list read as "none".
        let unknown = conflict_brief(7, "main", "agent/x", &[]);
        assert!(unknown.contains("has not said which files"), "{unknown}");
    }

    #[test]
    fn nobody_reviews_their_own_work() {
        assert!(may_review("rex", Some("ada")).is_ok());
        assert!(may_review("rex", None).is_ok(), "an unheld card is anybody's to read");

        let refused = may_review("ada", Some("ada")).expect_err("not your own");
        assert!(refused.contains("somebody else's job"), "{refused}");

        assert!(may_review("  ", Some("ada")).is_err(), "a review has to be signed");
    }

    #[test]
    fn a_verdict_is_read_however_it_is_written() {
        assert_eq!(Verdict::read("approve"), Some(Verdict::Approved));
        assert_eq!(Verdict::read("APPROVED"), Some(Verdict::Approved));
        assert_eq!(Verdict::read("request-changes"), Some(Verdict::ChangesRequested));
        assert_eq!(Verdict::read("changes requested"), Some(Verdict::ChangesRequested));
        assert_eq!(Verdict::read("comment"), Some(Verdict::Commented));

        assert_eq!(Verdict::read("lgtm"), None, "a word nobody defined is not a verdict");
        assert_eq!(Verdict::read(""), None);
    }

    #[test]
    fn only_asking_for_changes_hands_the_work_back() {
        assert!(Verdict::ChangesRequested.sends_it_back());
        assert!(!Verdict::Approved.sends_it_back());
        assert!(!Verdict::Commented.sends_it_back());
    }

    #[test]
    fn a_review_says_who_reached_it() {
        let said = review_comment("rex", Verdict::ChangesRequested, "The port probe is still racy.");

        assert!(said.starts_with("**rex** requested changes this."), "{said}");
        assert!(said.contains("The port probe is still racy."), "{said}");

        // A verdict with nothing said is still a verdict, not a dangling body.
        let bare = review_comment("rex", Verdict::Approved, "   ");
        assert_eq!(bare, "**rex** approved this.");
    }

    #[test]
    fn a_conflict_outranks_a_red_check() {
        // Rebasing replaces the result the checks ran against, so sending an
        // agent after the test first is sending it after an answer that is
        // about to change.
        let pull = open("CONFLICTING", "APPROVED", vec![check("test", "COMPLETED", "FAILURE")]);

        assert_eq!(where_it_stands(&pull, LONG_ENOUGH), Standing::Conflicted);
    }

    #[test]
    fn a_red_check_names_itself_and_goes_back_to_the_agent() {
        let pull = open(
            "MERGEABLE",
            "APPROVED",
            vec![check("lint", "COMPLETED", "SUCCESS"), check("test", "COMPLETED", "FAILURE")],
        );

        let standing = where_it_stands(&pull, LONG_ENOUGH);
        assert_eq!(standing, Standing::ChecksFailing { failed: vec!["test".to_owned()] });
        assert!(standing.goes_back_to_the_agent());
        assert!(in_a_line(&standing).contains("test"));
    }

    #[test]
    fn a_check_nobody_ran_is_not_a_failure() {
        for skipped in ["SKIPPED", "NEUTRAL", "CANCELLED", ""] {
            let pull = open("MERGEABLE", "APPROVED", vec![check("optional", "COMPLETED", skipped)]);

            assert_eq!(
                where_it_stands(&pull, LONG_ENOUGH),
                Standing::Ready,
                "{skipped} is not somebody's problem"
            );
        }
    }

    #[test]
    fn a_running_check_is_waited_on_rather_than_blamed() {
        let pull = open("MERGEABLE", "APPROVED", vec![check("test", "IN_PROGRESS", "")]);

        let standing = where_it_stands(&pull, LONG_ENOUGH);
        assert!(matches!(standing, Standing::Waiting { .. }));
        assert!(!standing.goes_back_to_the_agent(), "nobody has done anything wrong yet");
        assert!(in_a_line(&standing).contains("1 check is still running"));
    }

    #[test]
    fn a_commit_status_speaks_through_one_field() {
        let pending = Check { state: "PENDING".to_owned(), ..Check::default() };
        let failed = Check { state: "FAILURE".to_owned(), ..Check::default() };
        let passed = Check { state: "SUCCESS".to_owned(), ..Check::default() };

        assert!(!pending.is_finished());
        assert!(failed.is_failing());
        assert!(passed.is_finished() && !passed.is_failing());
    }

    #[test]
    fn waiting_on_a_review_is_not_the_agents_problem() {
        let pull = open("MERGEABLE", "REVIEW_REQUIRED", vec![]);

        let standing = where_it_stands(&pull, LONG_ENOUGH);
        assert!(!standing.goes_back_to_the_agent());
        assert!(in_a_line(&standing).contains("nobody has reviewed"));
    }

    #[test]
    fn a_pull_request_whose_checks_have_not_woken_up_is_not_ready() {
        // The exact state a real pull request was in twelve seconds after being
        // opened: mergeable, clean, no review asked for, and no checks reported
        // — while a workflow was being registered behind it.
        let fresh = PullState {
            number: 1,
            url: "https://github.com/corevider/ccdo/pull/1".to_owned(),
            state: "OPEN".to_owned(),
            mergeable: "MERGEABLE".to_owned(),
            merge_state: "CLEAN".to_owned(),
            base: "main".to_owned(),
            review: String::new(),
            checks: Vec::new(),
        };

        assert_eq!(
            where_it_stands(&fresh, 12),
            Standing::Waiting { why: "no checks have reported yet".to_owned() }
        );

        // And a repository that genuinely runs no checks is not held forever.
        assert_eq!(where_it_stands(&fresh, LONG_ENOUGH), Standing::Ready);
    }

    #[test]
    fn the_forge_has_the_last_word_on_ready() {
        // BEHIND is not here: it has a name of its own and an agent to give it
        // to. These are the ones where something is in the way that this cannot
        // name, and naming it wrongly would send somebody after nothing.
        for state in ["BLOCKED", "UNSTABLE", "HAS_HOOKS"] {
            let mut pull = open("MERGEABLE", "", vec![check("test", "COMPLETED", "SUCCESS")]);
            pull.merge_state = state.to_owned();

            let standing = where_it_stands(&pull, LONG_ENOUGH);
            assert!(
                matches!(standing, Standing::Waiting { .. }),
                "{state} is something in the way, whatever this can name"
            );
            assert!(!standing.goes_back_to_the_agent(), "{state} is nobody's fault yet");
        }
    }

    #[test]
    fn a_merge_github_has_not_worked_out_is_not_offered_as_ready() {
        let pull = open("UNKNOWN", "APPROVED", vec![]);

        assert!(matches!(where_it_stands(&pull, LONG_ENOUGH), Standing::Waiting { .. }));
    }

    #[test]
    fn what_gh_actually_prints_is_read() {
        let printed = r#"{
            "number": 7,
            "url": "https://github.com/o/r/pull/7",
            "state": "OPEN",
            "mergeable": "MERGEABLE",
            "mergeStateStatus": "BLOCKED",
            "reviewDecision": "REVIEW_REQUIRED",
            "statusCheckRollup": [
                {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"SUCCESS"},
                {"__typename":"StatusContext","context":"ci/other","state":"PENDING"}
            ]
        }"#;

        let pull: PullState = serde_json::from_str(printed).expect("gh's own shape");

        assert_eq!(pull.number, 7);
        assert_eq!(pull.checks.len(), 2);
        assert!(matches!(where_it_stands(&pull, LONG_ENOUGH), Standing::Waiting { .. }));
    }
}
