//! Notices shown by the desktop, for a person whose window is somewhere else.
//!
//! The bell in the window is enough for someone looking at the window. Someone
//! in another program, or with the window put away in the tray, hears about an
//! agent waiting on them only if the desktop says so.

use serde_json::Value;

use super::{ask_the_core, send_the_window_to, show_the_window, CoreEndpoint};

/// At most this many at once. A crew finishing a plan can raise a dozen notices
/// in a second, and a dozen popups is a reason to switch them off.
const AT_ONCE: usize = 3;

/// A notice worth showing outside the window.
#[derive(Debug, PartialEq)]
pub struct Alert {
    /// The notice it came from, so a click can mark it read. None for the line
    /// that stands in for the ones not shown.
    pub notice_id: Option<u64>,
    pub title: String,
    pub body: String,
    pub opens: Option<String>,
}

fn title_for(kind: &str) -> Option<&'static str> {
    match kind {
        "waiting" => Some("Waiting on you"),
        "trouble" => Some("Something went wrong"),
        "finished" => Some("Finished"),
        _ => None,
    }
}

fn alert_for(notice: &Value) -> Option<Alert> {
    let title = title_for(notice["kind"].as_str()?)?;
    let place = notice["repository_id"]
        .as_str()
        .or_else(|| notice["workspace_id"].as_str())
        .filter(|place| !place.is_empty());

    Some(Alert {
        notice_id: notice["id"].as_u64(),
        title: match place {
            Some(place) => format!("{title} · {place}"),
            None => title.to_owned(),
        },
        body: notice["text"].as_str().unwrap_or_default().to_owned(),
        opens: notice["opens"].as_str().map(str::to_owned),
    })
}

/// What arrived since the last reading, as alerts oldest first, and the id to
/// count from next time.
///
/// The first reading only sets the mark: a window starting up onto yesterday's
/// notices has no news. Notices live in the core's memory and are counted again
/// from one when it restarts, so a newest id below the mark starts the mark
/// again rather than waiting for the count to catch up. Word notices, notices
/// already read in the window and anything while the switch is off only move
/// the mark.
pub fn fresh(report: &Value, since: Option<u64>) -> (Vec<Alert>, Option<u64>) {
    let notices = report["notices"].as_array().map(Vec::as_slice).unwrap_or_default();
    let newest = notices.iter().filter_map(|notice| notice["id"].as_u64()).max().unwrap_or(0);

    let Some(since) = since else {
        return (Vec::new(), Some(newest));
    };
    if newest < since || report["desktop"].as_bool() == Some(false) {
        return (Vec::new(), Some(newest));
    }

    let mut arrived: Vec<Alert> = notices
        .iter()
        .filter(|notice| notice["id"].as_u64().is_some_and(|id| id > since))
        .filter(|notice| notice["seen"].as_bool() != Some(true))
        .filter_map(alert_for)
        .collect();
    arrived.reverse();

    if arrived.len() > AT_ONCE {
        let left_out = arrived.len() - (AT_ONCE - 1);
        arrived.drain(..left_out);
        arrived.push(Alert {
            notice_id: None,
            title: "Agentland".to_owned(),
            body: format!("{left_out} more notices are waiting under the bell"),
            opens: None,
        });
    }

    (arrived, Some(newest))
}

fn window_is_in_front(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;

    app.get_webview_window("main")
        .map(|window| window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false))
        .unwrap_or(false)
}

/// Where a click on a notice goes: the notice is read, and the window comes
/// forward onto the thing it was about.
fn go_to(app: &tauri::AppHandle, endpoint: &CoreEndpoint, alert: &Alert) {
    if let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
    {
        if let Some(id) = alert.notice_id {
            let _ = client
                .post(format!("http://{}:{}/notices", endpoint.host, endpoint.port))
                .header("x-auth-token", &endpoint.token)
                .json(&serde_json::json!({ "ids": [id] }))
                .send();
        }
        if let Some(opens) = alert.opens.as_deref() {
            send_the_window_to(&client, endpoint, opens);
        }
    }

    let for_the_window = app.clone();
    let _ = app.run_on_main_thread(move || show_the_window(&for_the_window));
}

fn show(app: &tauri::AppHandle, endpoint: &CoreEndpoint, alert: Alert) {
    let mut notification = notify_rust::Notification::new();
    notification.appname("Agentland").summary(&alert.title).body(&alert.body);

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        notification.action("default", "Open");
        match notification.show() {
            Ok(shown) => {
                let app = app.clone();
                let endpoint = endpoint.clone();
                std::thread::spawn(move || {
                    shown.wait_for_action(|action| {
                        if action == "default" {
                            go_to(&app, &endpoint, &alert);
                        }
                    });
                });
            }
            Err(error) => tracing::warn!(%error, "the desktop would not show a notice"),
        }
    }

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        let _ = (app, endpoint);
        if let Err(error) = notification.show() {
            tracing::warn!(%error, "the desktop would not show a notice");
        }
    }
}

/// Read the notices every few seconds and show the new ones, unless the window
/// is already in front, where the bell says the same thing.
pub fn watch(app: tauri::AppHandle, endpoint: CoreEndpoint) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(1500))
            .build()
        else {
            return;
        };

        let mut since = None;
        loop {
            if let Some(report) = ask_the_core(&client, &endpoint, "/notices?limit=50") {
                let (arrived, next) = fresh(&report, since);
                since = next;
                if !arrived.is_empty() && !window_is_in_front(&app) {
                    for alert in arrived {
                        show(&app, &endpoint, alert);
                    }
                }
            }

            std::thread::sleep(std::time::Duration::from_secs(4));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::fresh;
    use serde_json::json;

    fn notice(id: u64, kind: &str, text: &str) -> serde_json::Value {
        json!({
            "id": id, "kind": kind, "text": text, "seen": false,
            "repository_id": "shop", "workspace_id": null, "agent_id": "ada", "opens": "agent:ada",
        })
    }

    fn report(notices: Vec<serde_json::Value>) -> serde_json::Value {
        json!({ "notices": notices, "unseen": 0, "loud": false, "desktop": true })
    }

    #[test]
    fn the_first_reading_only_sets_the_mark() {
        let (arrived, mark) = fresh(&report(vec![notice(7, "waiting", "old news")]), None);
        assert!(arrived.is_empty());
        assert_eq!(mark, Some(7));
    }

    #[test]
    fn what_arrived_is_shown_oldest_first_and_words_are_not() {
        let now = report(vec![
            notice(10, "finished", "t4 is ready to merge"),
            notice(9, "word", "a note was written"),
            notice(8, "waiting", "Ada is holding a question open for you"),
            notice(7, "waiting", "already known"),
        ]);

        let (arrived, mark) = fresh(&now, Some(7));

        assert_eq!(mark, Some(10));
        let said: Vec<(&str, &str)> = arrived.iter().map(|alert| (alert.title.as_str(), alert.body.as_str())).collect();
        assert_eq!(
            said,
            vec![
                ("Waiting on you · shop", "Ada is holding a question open for you"),
                ("Finished · shop", "t4 is ready to merge"),
            ]
        );
        assert_eq!(arrived[0].notice_id, Some(8));
        assert_eq!(arrived[0].opens.as_deref(), Some("agent:ada"));
    }

    #[test]
    fn a_notice_read_in_the_window_is_not_shown_again() {
        let mut read = notice(8, "waiting", "seen already");
        read["seen"] = json!(true);

        let (arrived, _) = fresh(&report(vec![read]), Some(7));
        assert!(arrived.is_empty());
    }

    #[test]
    fn switched_off_nothing_is_shown_and_nothing_piles_up() {
        let mut off = report(vec![notice(8, "waiting", "Ada asks")]);
        off["desktop"] = json!(false);

        let (arrived, mark) = fresh(&off, Some(7));
        assert!(arrived.is_empty());
        assert_eq!(mark, Some(8), "turning it on later does not replay these");
    }

    #[test]
    fn a_restarted_core_starts_the_count_again() {
        let (arrived, mark) = fresh(&report(vec![notice(2, "waiting", "after a restart")]), Some(40));
        assert!(arrived.is_empty());
        assert_eq!(mark, Some(2));

        let (arrived, _) = fresh(&report(vec![notice(3, "trouble", "t2: tests fail"), notice(2, "waiting", "")]), mark);
        assert_eq!(arrived.len(), 1);
        assert_eq!(arrived[0].title, "Something went wrong · shop");
    }

    #[test]
    fn a_core_with_nothing_yet_still_counts_from_zero() {
        let (_, mark) = fresh(&report(Vec::new()), None);
        assert_eq!(mark, Some(0));

        let (arrived, _) = fresh(&report(vec![notice(1, "finished", "Plan finished: ship it")]), mark);
        assert_eq!(arrived.len(), 1);
    }

    #[test]
    fn a_burst_is_shown_as_a_few_and_a_count_of_the_rest() {
        let burst: Vec<serde_json::Value> = (1..=6).rev().map(|id| notice(id, "finished", &format!("step {id}"))).collect();

        let (arrived, _) = fresh(&report(burst), Some(0));

        let bodies: Vec<&str> = arrived.iter().map(|alert| alert.body.as_str()).collect();
        assert_eq!(bodies, vec!["step 5", "step 6", "4 more notices are waiting under the bell"]);
        assert_eq!(arrived[2].notice_id, None);
        assert_eq!(arrived[2].opens, None);
    }
}
