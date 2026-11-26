//! The palette's "Let another bot review a stopped chain.": whether another bot may review a chain
//! stopped at the hop limit (`setHopReview`, kept by the host for every app). Off until turned on.

use serde_json::{Value, json};

use super::{After, App};

impl App {
    /// Flips it; nothing before the `hello` says where it stands (or from a host without it).
    pub(super) fn toggle_hop_review(&mut self) {
        if let Some(on) = self.hop_review {
            self.call("setHopReview", json!({"enabled": !on}), After::HopReview);
        }
    }

    pub(super) fn hop_review_set(&mut self, v: &Value) {
        self.hop_review = v["enabled"].as_bool();
        self.flash(if self.hop_review == Some(true) {
            "Another bot can review a stopped chain"
        } else {
            "A stopped chain stays stopped"
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Action, Editor, Goto, GotoItem, test_app};

    #[tokio::test]
    async fn the_palette_hides_hop_review_until_the_host_advertises_it() {
        let mut app = test_app();
        let offered = |app: &super::super::App| {
            let g = Goto { query: Editor::with(""), filter: 0, cursor: 0 };
            app.goto_items(&g).iter().any(|item| matches!(item, GotoItem::Action(Action::HopReview)))
        };
        assert!(!offered(&app), "a host that does not advertise hopReview hides the row");
        app.hop_review = Some(false);
        assert!(offered(&app), "an advertised off switch still shows");
        app.hop_review = Some(true);
        assert!(offered(&app));
    }
}
