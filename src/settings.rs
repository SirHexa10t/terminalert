//! A topic's settings — said once, at the top of its file — and their defaults, which are the
//! project's own: the values `template.toml` writes, read from it, so that the format's reference
//! and the program can't disagree on them.

use std::ops::Range;
use std::sync::LazyLock;

use serde::Deserialize;
use toml::Spanned;

use crate::words;

/// `template.toml`: the files' format reference, every field at its default — and so the
/// defaults' source.
pub const TEMPLATE: &str = include_str!("../template.toml");

/// How soon a topic's reminders want doing: 1 to 5 in the file, a word when printed
/// ([`words::URGENCY`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Urgency(u8);

impl Urgency {
    /// The urgency `level` names, if it names one.
    pub fn new(level: i64) -> Option<Self> {
        let level = u8::try_from(level).ok()?;
        (1..=words::URGENCY.len()).contains(&usize::from(level)).then_some(Urgency(level))
    }

    /// Its number, as a file writes it.
    pub fn level(self) -> u8 {
        self.0
    }

    /// Its word, as the output prints it.
    pub fn word(self) -> &'static str {
        words::URGENCY[usize::from(self.0) - 1]
    }
}

/// A topic's settings, each resolved: the file's own where it writes one, the default where
/// not.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Its reminders come back, each by its timing (bills, birthdays) — or it is a to-do list.
    pub recurring: bool,
    /// How long ahead of its date a reminder starts reminding.
    pub notice: jiff::Span,
    /// A reminder keeps reminding past its date until it is marked — or lapses once it has
    /// passed.
    pub sticky: bool,
    /// Where the topic comes in the reminders' order.
    pub urgency: Urgency,
}

/// The project's defaults: every setting as `template.toml` writes it.
pub fn defaults() -> &'static Settings {
    static DEFAULTS: LazyLock<Settings> = LazyLock::new(|| {
        let written: Written = toml::from_str(TEMPLATE).expect("template.toml reads: tested");
        written.complete().expect("template.toml writes every setting, each valid: tested")
    });
    &DEFAULTS
}

/// What a file writes, before the defaults fill in what it leaves out: its settings, and its
/// table. The field names are the keys; a key that isn't one of them is refused, a misspelled
/// setting along with it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Written {
    recurring: Option<bool>,
    notice: Option<Spanned<String>>,
    sticky: Option<bool>,
    urgency: Option<Spanned<i64>>,
    pub(crate) pending: Option<Spanned<String>>,
}

/// A setting a file writes that can't be used, where it is, and why.
pub(crate) type Misset = (Range<usize>, String);

impl Written {
    /// The settings, the defaults filling in what the file leaves out — and in place of what it
    /// writes wrongly, which comes back as a problem each.
    pub(crate) fn settings(&self, defaults: &Settings) -> (Settings, Vec<Misset>) {
        let mut missets = Vec::new();
        let notice = self.notice.as_ref().map(spanned(|text: &String| notice(text)));
        let notice = or_default(notice, defaults.notice, &mut missets);
        let urgency = self.urgency.as_ref().map(spanned(|level: &i64| urgency(*level)));
        let urgency = or_default(urgency, defaults.urgency, &mut missets);
        let settings = Settings {
            recurring: self.recurring.unwrap_or(defaults.recurring),
            notice,
            sticky: self.sticky.unwrap_or(defaults.sticky),
            urgency,
        };
        (settings, missets)
    }

    /// The settings of a file that writes every one of them, each valid — the template.
    fn complete(&self) -> Result<Settings, String> {
        let missing = |name: &str| format!("no {name}");
        let notice = self.notice.as_ref().ok_or_else(|| missing("notice"))?;
        let urgency = self.urgency.as_ref().ok_or_else(|| missing("urgency"))?;
        Ok(Settings {
            recurring: self.recurring.ok_or_else(|| missing("recurring"))?,
            notice: self::notice(notice.get_ref())?,
            sticky: self.sticky.ok_or_else(|| missing("sticky"))?,
            urgency: self::urgency(*urgency.get_ref())?,
        })
    }
}

/// `read` applied to a spanned value, its failure placed where the value is.
fn spanned<T, U>(
    read: impl Fn(&T) -> Result<U, String>,
) -> impl Fn(&Spanned<T>) -> Result<U, Misset> {
    move |value| read(value.get_ref()).map_err(|why| (value.span(), why))
}

/// The value a file wrote, if it could be read; the default if not — the problem kept — or if it
/// wrote none.
fn or_default<T>(found: Option<Result<T, Misset>>, default: T, missets: &mut Vec<Misset>) -> T {
    match found {
        Some(Ok(value)) => value,
        Some(Err(misset)) => {
            missets.push(misset);
            default
        }
        None => default,
    }
}

/// A notice as a file writes it: a length — "3 days", "2 weeks", "12 hours" — never negative.
fn notice(text: &str) -> Result<jiff::Span, String> {
    let span: jiff::Span = text.parse().map_err(|_| {
        format!("can't read the notice {text:?}: write a length, like \"3 days\", \"2 weeks\" or \"12 hours\"")
    })?;
    if span.is_negative() {
        return Err(format!(
            "the notice {text:?} is negative: it's how long AHEAD of a reminder's date to start"
        ));
    }
    Ok(span)
}

/// An urgency as a file writes it: one of its numbers.
fn urgency(level: i64) -> Result<Urgency, String> {
    Urgency::new(level).ok_or_else(|| {
        let last = words::URGENCY.len();
        format!(
            "urgency {level} isn't one: it's 1 ({}) to {last} ({})",
            words::URGENCY[0],
            words::URGENCY[last - 1]
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The template writes every setting, each valid — the defaults the program runs on are
    /// those, exactly as the reference shows them.
    #[test]
    fn the_defaults_are_the_template_s_settings() {
        let defaults = defaults();
        assert!(!defaults.recurring);
        assert_eq!(defaults.notice.fieldwise(), jiff::Span::new().days(1).fieldwise());
        assert!(!defaults.sticky);
        assert_eq!(defaults.urgency.word(), "soon");
    }

    /// Urgency's numbers are 1 to 5, each printed as its word.
    #[test]
    fn urgency_runs_from_whenever_to_asap() {
        let words: Vec<&str> = (1..=5).map(|level| Urgency::new(level).unwrap().word()).collect();
        assert_eq!(words, ["whenever", "can-wait", "soon", "pressing", "ASAP"]);
        for outside in [0, 6, -1, 300] {
            assert_eq!(Urgency::new(outside), None, "{outside}");
        }
        assert!(Urgency::new(5) > Urgency::new(1), "the more urgent, the higher");
    }

    /// What a file leaves out comes from the defaults; what it writes wrongly comes from them
    /// too, and is reported where it was written.
    #[test]
    fn a_file_s_own_settings_stand_and_the_defaults_fill_the_rest() {
        let text = "notice = \"2 weeks\"\nurgency = 9\nsticky = true\n";
        let written: Written = toml::from_str(text).unwrap();
        let (settings, missets) = written.settings(defaults());
        assert_eq!(settings.notice.fieldwise(), jiff::Span::new().weeks(2).fieldwise());
        assert!(settings.sticky && !settings.recurring);
        assert_eq!(settings.urgency, defaults().urgency, "a wrong urgency falls back");
        assert_eq!(missets.len(), 1);
        assert_eq!(&text[missets[0].0.clone()], "9", "reported where it is written");
        assert!(missets[0].1.contains("1 (whenever) to 5 (ASAP)"), "{}", missets[0].1);
    }

    /// A notice is a length, and never a negative one.
    #[test]
    fn a_notice_is_a_length_ahead_of_the_date() {
        for good in ["1 day", "5 days", "2 weeks", "12 hours", "1 day 12 hours", "0 days"] {
            assert!(notice(good).is_ok(), "{good}");
        }
        assert!(notice("a week").unwrap_err().contains("\"3 days\""));
        assert!(notice("-2 days").unwrap_err().contains("negative"));
    }

    /// A misspelled setting is refused, not ignored.
    #[test]
    fn a_misspelled_setting_is_refused() {
        let refused = toml::from_str::<Written>("stiky = true\n").unwrap_err();
        assert!(refused.message().contains("stiky"), "{refused}");
    }
}
