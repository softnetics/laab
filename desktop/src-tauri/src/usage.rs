//! How often features are used, and the last few things the user did.
//!
//! Counts stay in memory and in the kv table, and go to PostHog once a day as
//! one `daily_usage` event with numbers only. Breadcrumbs are the last
//! [`CRUMBS`] action names, attached to error reports so they show what led
//! up to a problem. Neither holds ids, names or text.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::analytics::{self, Event, Status};
use crate::state::AppState;

const CRUMBS: usize = 20;
/// kv: the day the stored counts belong to (days since 1970, UTC).
const DAY: &str = "usage-day";
/// kv: the counts so far that day, as JSON.
const COUNTS: &str = "usage-counts";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    OpenedChat,
    SentText,
    SentSticker,
    SentFile,
    Reacted,
    Translated,
    OpenedLiff,
    Searched,
    CreatedFolder,
    Refreshed,
    OpenedDeepLink,
}

impl Action {
    pub const ALL: [Action; 11] = [
        Action::OpenedChat,
        Action::SentText,
        Action::SentSticker,
        Action::SentFile,
        Action::Reacted,
        Action::Translated,
        Action::OpenedLiff,
        Action::Searched,
        Action::CreatedFolder,
        Action::Refreshed,
        Action::OpenedDeepLink,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Action::OpenedChat => "opened_chat",
            Action::SentText => "sent_text",
            Action::SentSticker => "sent_sticker",
            Action::SentFile => "sent_file",
            Action::Reacted => "reacted",
            Action::Translated => "translated",
            Action::OpenedLiff => "opened_liff",
            Action::Searched => "searched",
            Action::CreatedFolder => "created_folder",
            Action::Refreshed => "refreshed",
            Action::OpenedDeepLink => "opened_deep_link",
        }
    }

    fn from_name(name: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.name() == name)
    }
}

static COUNTED: Mutex<BTreeMap<Action, u32>> = Mutex::new(BTreeMap::new());
static TRAIL: Mutex<VecDeque<Action>> = Mutex::new(VecDeque::new());

/// Notes that the user did `action`.
pub fn hit(action: Action) {
    *COUNTED.lock().unwrap_or_else(|e| e.into_inner()).entry(action).or_default() += 1;
    let mut trail = TRAIL.lock().unwrap_or_else(|e| e.into_inner());
    if trail.len() == CRUMBS {
        trail.pop_front();
    }
    trail.push_back(action);
}

/// The last actions, oldest first.
pub fn crumbs() -> Vec<&'static str> {
    TRAIL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|a| a.name())
        .collect()
}

fn today() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

/// `1970-01-01` style.
fn date(day: u64) -> String {
    // Civil-from-days (Howard Hinnant), enough for a label.
    let z = day as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// A rough size, so the number of chats can't single anyone out.
fn bucket(chats: usize) -> &'static str {
    match chats {
        0..=50 => "0-50",
        51..=200 => "51-200",
        201..=1000 => "201-1000",
        _ => "1000+",
    }
}

/// Moves counts from memory into the kv table, and sends the day before's
/// once a new day starts. Counts are dropped unless statistics are on.
pub fn flush(state: &AppState) {
    let counted = std::mem::take(&mut *COUNTED.lock().unwrap_or_else(|e| e.into_inner()));
    if analytics::status(state) != Status::On {
        let store = state.store();
        let _ = store.delete_kv(DAY);
        let _ = store.delete_kv(COUNTS);
        return;
    }
    let today = today();
    let (day, mut stored) = {
        let store = state.store();
        let day = store.get_kv(DAY).ok().flatten().and_then(|d| d.parse::<u64>().ok());
        let stored: BTreeMap<String, u32> = store
            .get_kv(COUNTS)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        (day, stored)
    };
    if let Some(day) = day.filter(|d| *d != today) {
        let counts = Action::ALL
            .into_iter()
            .map(|a| (a, stored.get(a.name()).copied().unwrap_or(0)))
            .collect();
        let chats = state.store().chats().map(|c| c.len()).unwrap_or(0);
        analytics::send(
            state,
            Event::DailyUsage {
                date: date(day),
                counts,
                chats: bucket(chats),
            },
        );
        stored.clear();
    }
    for (action, n) in counted {
        *stored.entry(action.name().to_string()).or_default() += n;
    }
    stored.retain(|k, _| Action::from_name(k).is_some());
    let store = state.store();
    let _ = store.set_kv(DAY, &today.to_string());
    let _ = store.set_kv(COUNTS, &serde_json::to_string(&stored).unwrap_or_default());
}

/// Flushes at launch and every 10 minutes.
pub fn start(state: std::sync::Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            flush(&state);
            tokio::time::sleep(Duration::from_secs(600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_buckets() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(20_391), "2025-10-30");
        assert_eq!(bucket(12), "0-50");
        assert_eq!(bucket(5000), "1000+");
    }

    #[test]
    fn crumbs_keep_the_last_twenty() {
        for _ in 0..25 {
            hit(Action::Searched);
        }
        hit(Action::SentText);
        let c = crumbs();
        assert_eq!(c.len(), CRUMBS);
        assert_eq!(c.last(), Some(&"sent_text"));
    }
}
