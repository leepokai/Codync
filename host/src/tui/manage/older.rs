//! A bot's threads that ended before its `work` carries them (`olderThreads`), listed under Recent on its Details page
//! (`work.rs` lays them out), a page at a time as the cursor reaches the last row.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use super::super::app::{App, Overlay, WorkTask};
use super::{DetailsPage, Reply};

/// A bot's older threads, as far as they were paged.
#[derive(Default)]
pub struct OlderThreads {
    /// Newest first.
    pub rows: Vec<WorkTask>,
    /// The host has more; the next page comes when the cursor reaches the last row.
    pub more: bool,
    /// A page is on its way.
    pub loading: bool,
    /// The request whose page this list takes ([`ask`]); a page asked before the list was dropped is let go.
    pub asked: u64,
}

/// A new request for a page of older threads, told apart from every earlier one.
fn ask() -> u64 {
    static ASKS: AtomicU64 = AtomicU64::new(1);
    ASKS.fetch_add(1, Ordering::Relaxed)
}

impl OlderThreads {
    /// A page's rows after these, each once.
    fn add(&mut self, page: &[Value]) {
        for task in page.iter().filter_map(WorkTask::parse) {
            if !self.rows.iter().any(|o| o.id == task.id) {
                self.rows.push(task);
            }
        }
    }

    /// What the next page asks before: the oldest row's end.
    fn before(&self) -> Option<i64> {
        self.rows.iter().filter_map(|t| t.ended_at).min()
    }
}

impl App {
    /// The cursor reached the page's last row: the next page of older threads comes, if the host has more.
    pub(super) fn more_older(&self, l: &mut DetailsPage, rows: usize) {
        if !l.older.more || l.older.loading || l.cursor + 1 < rows {
            return;
        }
        let Some(before) = l.older.before() else { return };
        l.older.loading = true;
        l.older.asked = ask();
        let body = json!({"botId": l.bot, "before": before});
        self.sheet("olderThreads", body, Reply::OlderThreads(l.bot.clone(), l.older.asked));
    }

    /// A page of `bot`'s older threads arrived for request `asked` (`None`: it couldn't come); the cursor stays on its row.
    pub(in crate::tui) fn older_threads(&mut self, bot: &str, asked: u64, page: Option<&Value>) {
        let (work, max_threads) = self.bots.get(bot).map_or((&[][..], None), |b| (&b.work, b.max_threads));
        for o in &mut self.overlays {
            let Overlay::Details(l) = o else { continue };
            if l.bot != bot || l.older.asked != asked {
                continue;
            }
            let picked = l.picked(work, max_threads).map(|row| row.id().to_owned());
            l.older.loading = false;
            l.older.more = page.is_some_and(|p| p["more"] == true);
            l.older.add(page.and_then(|p| p["threads"].as_array()).map_or(&[][..], Vec::as_slice));
            l.keep_cursor(picked.as_deref(), work, max_threads);
        }
    }

    /// `bot`'s older threads from the start: their first page is asked when the host keeps threads past a week
    /// (`hello.threadKeepDays`).
    pub(super) fn first_older(&self, bot: &str) -> OlderThreads {
        let loading = self.thread_keep_days.is_some();
        let asked = ask();
        if loading {
            self.sheet("olderThreads", json!({"botId": bot}), Reply::OlderThreads(bot.to_owned(), asked));
        }
        OlderThreads { loading, asked, ..Default::default() }
    }

    /// The computer's `threadKeepDays`, sent once it deleted threads (the choice the same or not): each open Details
    /// page lists its older threads again.
    pub(in crate::tui) fn relist_older(&mut self) {
        for i in 0..self.overlays.len() {
            let Overlay::Details(l) = &self.overlays[i] else { continue };
            let older = self.first_older(&l.bot);
            let Overlay::Details(l) = &mut self.overlays[i] else { continue };
            let (work, max_threads) = self.bots.get(&l.bot).map_or((&[][..], None), |b| (&b.work, b.max_threads));
            let picked = l.picked(work, max_threads).map(|row| row.id().to_owned());
            *l.older = older;
            l.keep_cursor(picked.as_deref(), work, max_threads);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::app::test_app;
    use super::super::DetailRow;
    use super::super::work::tests::bot_with;
    use super::*;

    fn thread(id: &str, ended_at: i64) -> Value {
        json!({"id": id, "title": id, "status": "done", "thread": format!("thread-{id}"), "endedAt": ended_at})
    }

    #[test]
    fn older_pages_follow_the_bots_recent_threads_once_each() {
        let mut app = test_app();
        bot_with(&mut app, &[("w", "done")]);
        app.overlays.push(Overlay::Details(DetailsPage { bot: "a".into(), ..Default::default() }));
        let first = json!({"threads": [thread("w", 300), thread("o1", 200)], "more": true});
        app.older_threads("a", 0, Some(&first));
        app.older_threads("a", 0, Some(&json!({"threads": [thread("o1", 200), thread("o2", 100)], "more": false})));
        let Some(Overlay::Details(l)) = app.overlays.last() else { panic!("the page is open") };
        let work = &app.bots["a"].work;
        let recent: Vec<&str> = l
            .sections(work, None)
            .into_iter()
            .filter(|s| s.title == "Recent")
            .flat_map(|s| s.rows)
            .filter_map(|r| if let DetailRow::Recent(t) = r { Some(t.id.as_str()) } else { None })
            .collect();
        assert_eq!(recent, ["w", "o1", "o2"], "the bot's own row first, then the older ones");
        assert_eq!((l.older.before(), l.older.more, l.older.loading), (Some(100), false, false));
    }

    #[tokio::test]
    async fn the_computers_keep_days_event_lists_older_threads_again() {
        let mut app = test_app();
        bot_with(&mut app, &[("w", "done")]);
        app.thread_keep_days = Some(30);
        app.overlays.push(Overlay::Details(DetailsPage { bot: "a".into(), ..Default::default() }));
        app.older_threads("a", 0, Some(&json!({"threads": [thread("o1", 200)], "more": false})));
        app.relist_older();
        app.older_threads("a", 0, Some(&json!({"threads": [thread("o1", 200)], "more": false})));
        let Some(Overlay::Details(l)) = app.overlays.last() else { panic!("the page is open") };
        assert!(l.older.rows.is_empty() && l.older.loading, "the deleted ones are gone until the page comes again");
        assert!(l.older.asked != 0, "a page asked before the cleanup is let go");
    }
}
