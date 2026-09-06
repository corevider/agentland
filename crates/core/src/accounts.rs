//! Which login an agent spends from.
//!
//! A subscription is not a base URL. A gateway can be pointed at with one, and
//! then the engine is spending API credit — not the Max or Pro plan somebody is
//! already paying for, which is an OAuth login bound to the provider's own
//! endpoint. So a second subscription is a second config folder and nothing
//! else, and everything here is about moving one engine's whole identity —
//! credentials, settings, transcripts — into a folder of our choosing.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// The variable that moves an engine's identity to a folder we name.
///
/// Only the ones known first-hand are here. `CLAUDE_CONFIG_DIR` is measured on
/// this machine: a fresh folder reports `loggedIn: false` from the engine's own
/// mouth while the home one stays signed in. An engine we cannot isolate is
/// offered no second account at all, which is honest — a guessed variable is
/// silently ignored, and then two rows claiming to be two people share one
/// login and one week's allowance.
pub fn config_home(engine_id: &str) -> Option<&'static str> {
    match engine_id {
        "claude" => Some("CLAUDE_CONFIG_DIR"),
        "codex" => Some("CODEX_HOME"),
        "gemini" => Some("GEMINI_CLI_HOME"),
        _ => None,
    }
}

/// Whether this engine can be asked who it is signed in as.
///
/// Claude Code and Codex both answer a status command. Gemini has none: it can
/// be given a folder of its own, and nothing in it will say whose folder it is.
/// A second Gemini login is therefore offered with that said out loud rather
/// than with a row that quietly claims to know.
pub fn can_be_asked(engine_id: &str) -> bool {
    status_command(engine_id).is_some()
}

/// How an engine is asked who it is signed in as.
///
/// Read from the engine every time rather than remembered here. A row that says
/// "signed in" because this app once opened a login pane is a row that lies the
/// moment a token expires or somebody signs out in another window.
pub fn status_command(engine_id: &str) -> Option<(&'static str, &'static [&'static str])> {
    match engine_id {
        "claude" => Some(("claude", &["auth", "status", "--json"])),
        "codex" => Some(("codex", &["login", "status"])),
        // Gemini has no status command. Guessing from the presence of a
        // credential file would be reading tea leaves in somebody's folder.
        _ => None,
    }
}

/// How an engine is asked to sign somebody in.
///
/// It opens a browser and waits, so it belongs in a pane a person can see and
/// answer — never in a process this app reads the output of and gives up on.
pub fn login_command(engine_id: &str) -> Option<(&'static str, &'static [&'static str])> {
    match engine_id {
        "claude" => Some(("claude", &["auth", "login"])),
        "codex" => Some(("codex", &["login"])),
        // Gemini signs in the first time it opens with nowhere to sign in from,
        // so the pane that does it is just the engine.
        "gemini" => Some(("gemini", &[])),
        _ => None,
    }
}

/// The folder inside an engine's config home that holds conversations, where
/// two accounts sharing it is the difference between resuming a session after a
/// switch and starting again from nothing.
///
/// Claude Code's is measured: `auth status` reports `projectsDirectory` under
/// the config home, and a symlink there is followed. The rest are left out
/// rather than guessed — a wrong name makes a folder nothing reads.
fn shared_folder(engine_id: &str) -> Option<&'static str> {
    match engine_id {
        "claude" => Some("projects"),
        _ => None,
    }
}

/// What a login's folder has to contain before the engine will look inside it.
///
/// Gemini's home variable names the folder that holds `.gemini`, rather than
/// `.gemini` itself — measured: a user-scope server added with the variable set
/// landed in `$GEMINI_CLI_HOME/.gemini/settings.json` and the real home was
/// never created.
fn inner_folder(engine_id: &str) -> Option<&'static str> {
    match engine_id {
        "gemini" => Some(".gemini"),
        _ => None,
    }
}

/// Whether this engine can hold more than one login on one machine.
pub fn can_hold_accounts(engine_id: &str) -> bool {
    config_home(engine_id).is_some()
}

/// A login on an engine, as the engine describes it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Account {
    pub engine_id: String,
    pub label: String,
    /// What the engine said, not what this app hoped.
    pub signed_in: bool,
    /// Who the engine says this is. None means the engine did not say, which is
    /// not the same as nobody.
    pub who: Option<String>,
    /// The plan the engine names — `max`, `pro`, an API key. None where it says
    /// nothing.
    pub plan: Option<String>,
    /// Whether this engine can be asked at all. False means `signed_in` is not
    /// a no — it is a silence, and the panel says so rather than guessing.
    pub askable: bool,
}

/// What an engine's status output says about a login.
///
/// Pure, because it is the part worth testing: the engines are not on every
/// machine that runs the tests, and an output shape that changes should fail
/// here rather than in a pane at midnight.
pub fn reading(engine_id: &str, said: &str) -> (bool, Option<String>, Option<String>) {
    match engine_id {
        "claude" => match serde_json::from_str::<serde_json::Value>(said) {
            Ok(held) => (
                held.get("loggedIn").and_then(serde_json::Value::as_bool).unwrap_or(false),
                held.get("email").and_then(serde_json::Value::as_str).map(str::to_owned),
                held.get("subscriptionType")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            ),
            Err(_) => (false, None, None),
        },
        _ => {
            let lowered = said.to_lowercase();
            let signed_in = lowered.contains("logged in") && !lowered.contains("not logged in");
            let who = said
                .split_whitespace()
                .find(|word| word.contains('@') && word.contains('.'))
                .map(|word| word.trim_matches(|c: char| !c.is_ascii_graphic() || c == ',').to_owned());

            (signed_in, who, None)
        }
    }
}

/// A label as it appears on disk. Two labels that differ only in punctuation are
/// one folder, so `Work Account` and `work-account` cannot become two rows
/// pointing at the same credentials.
pub fn slugify(label: &str) -> String {
    let slug: String = label
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    slug.split('-').filter(|piece| !piece.is_empty()).collect::<Vec<_>>().join("-")
}

fn engine_folder(data_dir: &Path, engine_id: &str) -> PathBuf {
    data_dir.join("accounts").join(slugify(engine_id))
}

/// Where one login's config home lives.
pub fn dir(data_dir: &Path, engine_id: &str, label: &str) -> PathBuf {
    engine_folder(data_dir, engine_id).join(slugify(label))
}

/// The variable and folder to hand a pane so it starts as this login.
///
/// None where nobody chose an account, or where the engine has no folder we
/// know how to move — in both cases the pane starts as whoever the machine is
/// already signed in as, which is the behaviour that existed before any of
/// this.
pub fn env_for(data_dir: &Path, engine_id: &str, label: Option<&str>) -> Option<(String, String)> {
    let label = label.map(str::trim).filter(|held| !held.is_empty())?;
    let variable = config_home(engine_id)?;
    let folder = dir(data_dir, engine_id, label);

    folder
        .is_dir()
        .then(|| (variable.to_owned(), folder.to_string_lossy().into_owned()))
}

/// Make a folder for a login, ready for the engine's own sign-in to fill.
///
/// Adding an account creates nothing that claims to be signed in. It creates an
/// empty identity; the engine's own login is what puts a credential in it, and
/// the engine's own status is what says so afterwards.
pub fn add(data_dir: &Path, engine_id: &str, label: &str) -> Result<PathBuf> {
    if !can_hold_accounts(engine_id) {
        bail!("{engine_id} keeps no config folder Agentland knows how to move, so it can hold one login only");
    }

    let slug = slugify(label);
    if slug.is_empty() {
        bail!("an account needs a name to be told apart from the other one");
    }

    let folder = dir(data_dir, engine_id, &slug);
    fs::create_dir_all(&folder)?;

    if let Some(inner) = inner_folder(engine_id) {
        fs::create_dir_all(folder.join(inner))?;
    }

    share_conversations(data_dir, engine_id, &folder);

    Ok(folder)
}

/// Point this login's conversations at the ones its engine's other logins use.
///
/// Without it a switch is amnesia: the config home moves, the transcripts move
/// with it, and `--continue` in the same worktree finds nothing. Shared, the
/// work carries across a switch — which is the whole point of having a second
/// subscription rather than a second computer.
///
/// Best effort, and deliberately so. A machine that refuses symlinks — Windows
/// without developer mode — gets an account that works and forgets, rather than
/// an account it cannot create.
fn share_conversations(data_dir: &Path, engine_id: &str, folder: &Path) {
    let Some(name) = shared_folder(engine_id) else {
        return;
    };

    let shared = engine_folder(data_dir, engine_id).join("conversations");
    if fs::create_dir_all(&shared).is_err() {
        return;
    }

    let link = folder.join(name);
    if link.exists() || fs::symlink_metadata(&link).is_ok() {
        return;
    }

    let _ = symlink_folder(&shared, &link);
}

#[cfg(unix)]
fn symlink_folder(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink_folder(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

/// Forget a login: the folder goes, and the credential in it goes with it.
///
/// This signs the account out of this machine. It is not a way to stop using an
/// account for a while — that is choosing a different one on the agent.
pub fn forget(data_dir: &Path, engine_id: &str, label: &str) -> Result<()> {
    let folder = dir(data_dir, engine_id, label);
    if !folder.is_dir() {
        bail!("no account called {label} on {engine_id}");
    }

    fs::remove_dir_all(&folder)?;
    Ok(())
}

/// What the engine says about one login, asked now.
pub fn status_of(data_dir: &Path, engine_id: &str, label: &str) -> Account {
    let label = slugify(label);
    let folder = dir(data_dir, engine_id, &label);

    let said = status_command(engine_id).and_then(|(command, args)| {
        let mut running = crate::exec::command(command);
        running.args(args);

        if let Some(variable) = config_home(engine_id) {
            running.env(variable, &folder);
        }

        running
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    });

    let (signed_in, who, plan) = said
        .as_deref()
        .map(|said| reading(engine_id, said))
        .unwrap_or((false, None, None));

    Account {
        engine_id: engine_id.to_owned(),
        label,
        signed_in,
        who,
        plan,
        askable: can_be_asked(engine_id),
    }
}

/// Every login on one engine, in the order they were named.
pub fn list(data_dir: &Path, engine_id: &str) -> Vec<Account> {
    let mut labels: Vec<String> = fs::read_dir(engine_folder(data_dir, engine_id))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name != "conversations")
        .collect();

    labels.sort();
    labels
        .iter()
        .map(|label| status_of(data_dir, engine_id, label))
        .collect()
}

/// The engines that can be given a folder of their own.
///
/// Named rather than discovered, so listing the logins somebody holds costs a
/// folder read instead of asking every engine on the machine what version it is.
pub const CAN_HOLD: &[&str] = &["claude", "codex", "gemini"];

/// Every login this machine holds.
///
/// Including logins on an engine that is not installed right now. A folder with
/// a credential in it does not stop existing because its engine was uninstalled
/// this morning, and a row that vanishes is a row nobody can forget on purpose.
pub fn all(data_dir: &Path) -> Vec<Account> {
    CAN_HOLD.iter().flat_map(|engine| list(data_dir, engine)).collect()
}

/// Another login on the same engine that could take this work on.
///
/// Signed in, not the one that ran out, and — where the caller knows — not one
/// whose own week is spent either. Order is the order on disk, so the answer is
/// the same every time rather than whichever the filesystem felt like.
pub fn stand_in(
    data_dir: &Path,
    engine_id: &str,
    spent: Option<&str>,
    exhausted: &dyn Fn(&str) -> bool,
) -> Option<Account> {
    let spent = spent.map(slugify);

    list(data_dir, engine_id)
        .into_iter()
        .filter(|account| account.signed_in)
        .filter(|account| Some(&account.label) != spent.as_ref())
        .find(|account| !exhausted(&account.label))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentland-accounts-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a scratch folder");
        dir
    }

    #[test]
    fn a_subscription_is_a_folder_not_a_base_url() {
        assert_eq!(config_home("claude"), Some("CLAUDE_CONFIG_DIR"));
        assert_eq!(config_home("codex"), Some("CODEX_HOME"));
    }

    #[test]
    fn the_named_engines_are_exactly_the_ones_with_a_folder_variable() {
        for engine in CAN_HOLD {
            assert!(can_hold_accounts(engine), "{engine} is named but has no variable");
        }

        for engine in ["cursor-agent", "crush", "goose", "opencode", "qwen"] {
            assert!(
                !CAN_HOLD.contains(&engine),
                "{engine} has no variable and must not be named"
            );
        }
    }

    #[test]
    fn gemini_gets_a_folder_but_cannot_be_asked_whose_it_is() {
        assert_eq!(config_home("gemini"), Some("GEMINI_CLI_HOME"));
        assert!(can_hold_accounts("gemini"));

        // It has no status command, so a row about it is a silence rather than
        // a no, and the panel has to say which.
        assert!(!can_be_asked("gemini"));
        assert!(can_be_asked("claude") && can_be_asked("codex"));
    }

    #[test]
    fn a_gemini_login_is_given_the_folder_the_engine_looks_inside() {
        let data = scratch("inner");
        let folder = add(&data, "gemini", "second").expect("the folder is made");

        // The variable names the folder that holds `.gemini`, not `.gemini`
        // itself, so the inner one is there before the engine goes looking.
        assert!(folder.join(".gemini").is_dir(), "{folder:?}");

        let _ = fs::remove_dir_all(&data);
    }

    #[test]
    fn cursor_holds_one_login_because_nothing_moves_its_home() {
        assert_eq!(config_home("cursor-agent"), None);
        assert!(!can_hold_accounts("cursor-agent"));
    }

    #[test]
    fn an_engine_we_cannot_isolate_is_offered_no_second_login() {
        assert_eq!(config_home("crush"), None);
        assert!(!can_hold_accounts("crush"));
    }

    #[test]
    fn claude_says_who_it_is_and_what_it_pays_for() {
        let said = r#"{"loggedIn":true,"authMethod":"claude.ai","email":"somebody@example.com","subscriptionType":"max"}"#;
        let (signed_in, who, plan) = reading("claude", said);

        assert!(signed_in);
        assert_eq!(who.as_deref(), Some("somebody@example.com"));
        assert_eq!(plan.as_deref(), Some("max"));
    }

    #[test]
    fn a_fresh_folder_is_signed_in_as_nobody() {
        let said = r#"{"loggedIn":false,"authMethod":"none"}"#;
        assert_eq!(reading("claude", said), (false, None, None));
    }

    #[test]
    fn output_that_is_not_the_shape_we_know_claims_nothing() {
        let (signed_in, who, plan) = reading("claude", "command not found");

        assert!(!signed_in, "an unreadable answer is not a yes");
        assert!(who.is_none() && plan.is_none());
    }

    #[test]
    fn not_logged_in_is_read_as_the_no_it_is() {
        assert_eq!(reading("codex", "Not logged in").0, false);
        assert_eq!(reading("codex", "Logged in using ChatGPT").0, true);
    }

    #[test]
    fn two_labels_that_differ_only_in_punctuation_are_one_folder() {
        assert_eq!(slugify("Work Account"), "work-account");
        assert_eq!(slugify("work_account"), "work-account");
        assert_eq!(slugify("  Second  "), "second");
    }

    #[test]
    fn adding_an_account_makes_an_identity_that_claims_nothing() {
        let data = scratch("adding");
        let folder = add(&data, "claude", "Second").expect("the folder is made");

        assert!(folder.is_dir());
        assert!(folder.ends_with("second"));

        let held = list(&data, "claude");
        assert_eq!(held.len(), 1, "one account, not the shared conversations folder too");
        assert!(!held[0].signed_in, "nothing signed it in");

        let _ = fs::remove_dir_all(&data);
    }

    #[test]
    fn an_engine_that_cannot_hold_two_logins_refuses_to_pretend() {
        let data = scratch("refusing");
        let error = add(&data, "crush", "second").expect_err("crush has no folder we can move");

        assert!(format!("{error}").contains("one login only"), "{error}");
        let _ = fs::remove_dir_all(&data);
    }

    #[test]
    fn a_pane_is_handed_the_folder_only_when_the_account_is_really_there() {
        let data = scratch("handing");

        assert_eq!(env_for(&data, "claude", Some("second")), None, "no folder, no promise");

        add(&data, "claude", "second").expect("the folder is made");
        let (variable, folder) = env_for(&data, "claude", Some("second")).expect("now it is there");

        assert_eq!(variable, "CLAUDE_CONFIG_DIR");
        assert!(folder.ends_with("second"), "{folder}");

        assert_eq!(env_for(&data, "claude", None), None, "nobody chose an account");
        assert_eq!(env_for(&data, "claude", Some("  ")), None, "a blank is not a choice");

        let _ = fs::remove_dir_all(&data);
    }

    #[test]
    fn a_login_that_cannot_be_isolated_is_never_handed_a_folder() {
        let data = scratch("unisolated");
        let folder = dir(&data, "crush", "second");
        fs::create_dir_all(&folder).expect("even if somebody made one by hand");

        assert_eq!(env_for(&data, "crush", Some("second")), None);
        let _ = fs::remove_dir_all(&data);
    }

    #[cfg(unix)]
    #[test]
    fn two_logins_on_one_engine_read_the_same_conversations() {
        let data = scratch("conversations");
        let first = add(&data, "claude", "first").expect("the first folder");
        let second = add(&data, "claude", "second").expect("the second folder");

        fs::write(first.join("projects").join("a-session.jsonl"), "{}").expect("written as the first");

        assert!(
            second.join("projects").join("a-session.jsonl").is_file(),
            "the second login sees what the first said, or a switch is amnesia"
        );

        let _ = fs::remove_dir_all(&data);
    }

    #[test]
    fn forgetting_an_account_that_was_never_there_says_so() {
        let data = scratch("forgetting");
        let error = forget(&data, "claude", "nobody").expect_err("there is no such account");

        assert!(format!("{error}").contains("no account called nobody"), "{error}");
        let _ = fs::remove_dir_all(&data);
    }

    #[test]
    fn a_stand_in_is_signed_in_and_is_not_the_one_that_ran_out() {
        let data = scratch("standing-in");
        add(&data, "claude", "first").expect("the first folder");
        add(&data, "claude", "second").expect("the second folder");

        // Nothing here is signed in — the engine says so — and an account
        // nobody is signed in as cannot take work on.
        assert_eq!(stand_in(&data, "claude", Some("first"), &|_| false), None);

        let _ = fs::remove_dir_all(&data);
    }
}
