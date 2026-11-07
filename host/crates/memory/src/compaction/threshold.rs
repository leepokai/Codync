//! The user's choice of when a conversation is compacted, its wire form, and the sizes a window allows.

use serde::{Deserialize, Serialize};

/// By default, compact past three quarters of the window or this many tokens, whichever is less: the summary needs
/// room, and a hot cache makes reading the history cheap.
const TURN_END_CAP: u64 = 200_000;
/// A size the user sets is a multiple of this...
const STEP: u64 = 10_000;
/// ...at least this: the summary and the opening alone take some 20k, so less would compact every few turns...
const MIN_TOKENS: u64 = 50_000;
/// ...and leaves this much of the window to the compaction turn: its prompt and a summary of about 20k.
const HEADROOM: u64 = 40_000;
/// The sizes a menu lists are multiples of this; any other size is the user's to type.
const PRESET_STEP: u64 = 100_000;

/// When the caller compacts a conversation: the user's choice. On the wire `"early"`, `"nearLimit"`, `"off"`, or the
/// size in tokens; anything else reads as the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Wire", into = "Wire")]
pub enum Threshold {
    /// At three quarters of the window or 200k, whichever is less; idle compaction and rollovers past 100k.
    #[default]
    Early,
    /// At the largest size the window allows ([`sizes`]), never sooner than [`Threshold::Early`].
    NearLimit,
    /// Never: the harness's own compaction, when a turn fills the window, is all there is.
    Off,
    /// At this many tokens, held to the [`sizes`] the window allows.
    Tokens(u64),
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Wire {
    Tokens(u64),
    Name(Name),
    /// A value from a newer caller.
    #[serde(skip_serializing)]
    Other(serde::de::IgnoredAny),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Name {
    Early,
    NearLimit,
    Off,
}

impl From<Wire> for Threshold {
    fn from(wire: Wire) -> Self {
        match wire {
            Wire::Tokens(n) => Self::Tokens(n),
            Wire::Name(Name::Early) | Wire::Other(_) => Self::Early,
            Wire::Name(Name::NearLimit) => Self::NearLimit,
            Wire::Name(Name::Off) => Self::Off,
        }
    }
}

impl From<Threshold> for Wire {
    fn from(threshold: Threshold) -> Self {
        match threshold {
            Threshold::Early => Self::Name(Name::Early),
            Threshold::NearLimit => Self::Name(Name::NearLimit),
            Threshold::Off => Self::Name(Name::Off),
            Threshold::Tokens(n) => Self::Tokens(n),
        }
    }
}

impl Threshold {
    /// Its name without the size, for analytics.
    pub fn kind(self) -> &'static str {
        match self {
            Self::Early => "early",
            Self::NearLimit => "nearLimit",
            Self::Off => "off",
            Self::Tokens(_) => "tokens",
        }
    }

    /// Whether the user may set it: a size is a multiple of 10k, at least 50k. Its upper bound is the window's, which
    /// it is held to as the conversation runs, since the window can change with the model.
    pub fn check(self) -> Result<(), String> {
        match self {
            Self::Tokens(n) if n < MIN_TOKENS || n % STEP != 0 => {
                Err(format!("compaction size must be a multiple of {STEP} tokens, at least {MIN_TOKENS}"))
            }
            _ => Ok(()),
        }
    }

    /// The context use that compacts a conversation in a window of `window` tokens at the end of a turn; `None` when
    /// the user turned that off. A window too small for a size gets the default's.
    pub(super) fn trigger(self, window: u64) -> Option<u64> {
        let early = (window / 4 * 3).min(TURN_END_CAP);
        let max = max_tokens(window);
        match self {
            Self::Early => Some(early),
            Self::NearLimit => Some(max.map_or(early, |max| max.max(early))),
            Self::Off => None,
            Self::Tokens(n) => Some(max.map_or(early, |max| n.clamp(MIN_TOKENS, max))),
        }
    }
}

/// The sizes a user may set in one window: `min` to `max` tokens by `step`, and the few a menu lists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sizes {
    pub min: u64,
    pub max: u64,
    pub step: u64,
    pub presets: Vec<u64>,
}

impl Sizes {
    /// A size typed as `tokens`, as one this window takes: to the nearest step, within `min` and `max`.
    pub fn fit(&self, tokens: u64) -> u64 {
        let step = self.step.max(1);
        (tokens.saturating_add(step / 2) / step * step).max(self.min).min(self.max)
    }
}

/// The sizes a window of `window` tokens allows; `None` for a window too small to offer one.
pub fn sizes(window: u64) -> Option<Sizes> {
    max_tokens(window).map(|max| Sizes {
        min: MIN_TOKENS,
        max,
        step: STEP,
        presets: (1..=max / PRESET_STEP).map(|n| n * PRESET_STEP).collect(),
    })
}

/// The largest size a window allows, leaving the compaction turn its headroom.
fn max_tokens(window: u64) -> Option<u64> {
    let max = window.saturating_sub(HEADROOM) / STEP * STEP;
    (max >= MIN_TOKENS).then_some(max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_threshold_reads_back_as_written() {
        for (threshold, wire) in [
            (Threshold::Early, json!("early")),
            (Threshold::NearLimit, json!("nearLimit")),
            (Threshold::Off, json!("off")),
            (Threshold::Tokens(120_000), json!(120_000)),
        ] {
            assert_eq!(serde_json::to_value(threshold).unwrap(), wire);
            assert_eq!(serde_json::from_value::<Threshold>(wire).unwrap(), threshold);
        }
        assert_eq!(serde_json::from_value::<Threshold>(json!("later")).unwrap(), Threshold::Early, "a newer app's");
    }

    #[test]
    fn a_window_offers_sizes_in_steps_short_of_its_headroom() {
        let million = sizes(1_000_000).expect("a 1M window offers sizes");
        assert_eq!((million.min, million.max, million.step), (50_000, 960_000, 10_000));
        assert_eq!(million.presets, (1..=9).map(|n| n * 100_000).collect::<Vec<_>>(), "100K to 900K");
        assert_eq!(sizes(272_000).map(|s| (s.max, s.presets)), Some((230_000, vec![100_000, 200_000])));
        assert_eq!(sizes(130_000).map(|s| s.presets), Some(vec![]), "90K at most: only a size of one's own");
        assert_eq!([354_000, 355_000, 10_000, 5_000_000].map(|n| million.fit(n)), [350_000, 360_000, 50_000, 960_000]);
        assert_eq!(sizes(64_000), None);
        for ok in [Threshold::NearLimit, Threshold::Tokens(50_000), Threshold::Tokens(2_000_000)] {
            assert!(ok.check().is_ok(), "{ok:?}");
        }
        for bad in [40_000, 125_000] {
            assert!(Threshold::Tokens(bad).check().is_err(), "{bad}");
        }
    }

    #[test]
    fn near_the_limit_is_never_sooner_than_early() {
        assert_eq!(Threshold::NearLimit.trigger(1_000_000), Some(960_000));
        assert_eq!(Threshold::NearLimit.trigger(128_000), Some(96_000), "a small window: three quarters of it");
        assert_eq!(Threshold::NearLimit.trigger(64_000), Some(48_000), "too small for a size: the default's");
    }
}
