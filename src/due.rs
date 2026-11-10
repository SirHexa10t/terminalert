//! What is due at a moment: each reminder's state, and the alerts the due ones make — listed one
//! by one, or counted per topic — in the order they're shown.
//!
//! A reminder shows from its notice ahead of its date. On the date it is due today; after it, a
//! sticky reminder is overdue until it is marked, and any other lapses. A chore is pending until it
//! is done. Sticky reminders and chores are counted per topic; the rest are listed.

use std::cmp::Ordering;

use jiff::Zoned;

use crate::done::{Entry, Record};
use crate::settings::Settings;
use crate::timing::{Occurrence, Timing};
use crate::topic::{Reminder, Topic};

/// Whether the most urgent topics come first — the reminders' order, which way it runs.
const MOST_URGENT_FIRST: bool = true;

/// What a reminder is, at a moment.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// Nothing to say: not within its notice yet, lapsed, or handled.
    Quiet,
    /// Its date is coming, within its notice.
    Upcoming(Occurrence),
    /// Its date is now: the day itself, for a whole-day one.
    Today(Occurrence),
    /// Its date has passed, and it isn't marked: a sticky reminder.
    Overdue(Occurrence),
    /// A chore: pending until it is done.
    Pending,
}

/// What `reminder` is at `now`, under its topic's `settings` — `round` being what its record holds
/// for it, if it is a recurring reminder with a round marked.
pub fn state(
    reminder: &Reminder,
    settings: &Settings,
    round: Option<&Entry>,
    now: &Zoned,
) -> State {
    if matches!(reminder.timing, Timing::None) {
        return State::Pending;
    }
    let handled = |occurrence: &Occurrence| {
        round.and_then(|entry| entry.due.as_deref()).is_some_and(|key| occurrence.handled_by(key))
    };
    let (latest, next) = reminder.timing.around(now);
    if let Some(latest) = latest
        && !handled(&latest)
    {
        if *now < latest.end() {
            return State::Today(latest);
        }
        if settings.sticky {
            return State::Overdue(latest);
        }
    }
    if let Some(next) = next
        && !handled(&next)
        && within_notice(&next, settings.notice, now)
    {
        return State::Upcoming(next);
    }
    State::Quiet
}

/// Whether `now` is within `notice` before `occurrence` — always, for a notice too long to count
/// back.
fn within_notice(occurrence: &Occurrence, notice: jiff::Span, now: &Zoned) -> bool {
    match occurrence.start.checked_sub(notice) {
        Ok(from) => *now >= from,
        Err(_) => true,
    }
}

/// The round a recurring `reminder` is marked for at `now`: the one showing now — today's,
/// overdue, or coming — or else the first not handled yet, marked ahead of time.
pub fn round_to_mark(
    reminder: &Reminder,
    settings: &Settings,
    round: Option<&Entry>,
    now: &Zoned,
) -> Option<Occurrence> {
    match state(reminder, settings, round, now) {
        State::Today(occurrence) | State::Overdue(occurrence) | State::Upcoming(occurrence) => {
            Some(occurrence)
        }
        State::Pending => None,
        State::Quiet => {
            let mut from = now.clone();
            loop {
                let next = reminder.timing.around(&from).1?;
                let handled = round
                    .and_then(|entry| entry.due.as_deref())
                    .is_some_and(|key| next.handled_by(key));
                if !handled {
                    return Some(next);
                }
                from = next.start;
            }
        }
    }
}

/// A reminder with something to say.
#[derive(Debug)]
pub struct Alert<'a> {
    pub topic: &'a Topic,
    pub reminder: &'a Reminder,
    pub state: State,
    /// In a yearly cycle ([`Topic::is_yearly_cycle`]), for a reminder written with its year: the
    /// years its date reaches — counted up to the date alerted of, so that a 30th anniversary says
    /// so ahead of the day.
    pub years: Option<i16>,
}

/// A topic's counted reminders: its sticky reminders with something to say, and its chores.
#[derive(Debug)]
pub struct Count<'a> {
    pub topic: &'a Topic,
    pub alerts: Vec<Alert<'a>>,
}

impl Count<'_> {
    /// How many of them are past their date.
    pub fn overdue(&self) -> usize {
        self.alerts.iter().filter(|alert| matches!(alert.state, State::Overdue(_))).count()
    }
}

/// Everything due at a moment, in the order it is shown.
#[derive(Debug, Default)]
pub struct Due<'a> {
    /// The reminders shown one by one.
    pub listed: Vec<Alert<'a>>,
    /// The reminders shown as a count, per topic.
    pub counted: Vec<Count<'a>>,
}

impl Due<'_> {
    /// Whether there's nothing to say.
    pub fn is_empty(&self) -> bool {
        self.listed.is_empty() && self.counted.is_empty()
    }
}

/// What is due in `shelf` — each topic with its record — at `now`.
pub fn due<'a>(shelf: &'a [(Topic, Record)], now: &Zoned) -> Due<'a> {
    let mut order: Vec<&(Topic, Record)> = shelf.iter().collect();
    order.sort_by(|(a, _), (b, _)| by_urgency(a, b));
    let mut due = Due::default();
    for (topic, record) in order {
        let yearly = topic.is_yearly_cycle(now.date());
        let mut counted = Vec::new();
        for reminder in &topic.reminders {
            let state = state(reminder, &topic.settings, record.round(&reminder.what), now);
            let years = if yearly { years_at(reminder, &state) } else { None };
            let alert = Alert { topic, reminder, state, years };
            match alert.state {
                State::Quiet => {}
                State::Pending | State::Overdue(_) => counted.push(alert),
                _ if topic.settings.sticky => counted.push(alert),
                _ => due.listed.push(alert),
            }
        }
        if !counted.is_empty() {
            due.counted.push(Count { topic, alerts: counted });
        }
    }
    due
}

/// The years `reminder`'s date reaches at the date `state` alerts of, from the date its year was
/// written with — none below one, and none for a reminder written without its year.
fn years_at(reminder: &Reminder, state: &State) -> Option<i16> {
    let since = reminder.timing.since()?;
    let (State::Upcoming(at) | State::Today(at)) = state else { return None };
    let years = at.start.date().year() - since.year();
    (years >= 1).then_some(years)
}

/// The topics' order: by urgency, then by name.
fn by_urgency(a: &Topic, b: &Topic) -> Ordering {
    let urgency = a.settings.urgency.cmp(&b.settings.urgency);
    let urgency = if MOST_URGENT_FIRST { urgency.reverse() } else { urgency };
    urgency.then_with(|| a.name.cmp(&b.name))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::done::How;

    fn at(text: &str) -> Zoned {
        format!("{text}[Europe/London]").parse().unwrap()
    }

    fn topic(name: &str, text: &str) -> Topic {
        let topic = Topic::parse(Path::new(&format!("/r/{name}.toml")), text.to_owned());
        assert!(topic.problems.is_empty(), "{name}: {:?}", topic.problems);
        topic
    }

    fn round(reminder: &str, due: &str) -> Entry {
        Entry {
            reminder: reminder.to_owned(),
            marked: String::new(),
            how: How::Done,
            due: Some(due.to_owned()),
            row: None,
        }
    }

    fn state_at(topic: &Topic, round: Option<&Entry>, now: &str) -> State {
        state(&topic.reminders[0], &topic.settings, round, &at(now))
    }

    fn kind(state: &State) -> String {
        match state {
            State::Quiet => "quiet".into(),
            State::Upcoming(at) => format!("upcoming {}", at.key()),
            State::Today(at) => format!("today {}", at.key()),
            State::Overdue(at) => format!("overdue {}", at.key()),
            State::Pending => "pending".into(),
        }
    }

    /// A birthday reminds through its notice, is due on its day, and lapses after it.
    #[test]
    fn a_lapsing_reminder_shows_through_its_notice_and_its_day() {
        let birthday = topic(
            "birthday",
            "recurring = true\nnotice = \"1 week\"\npending = '''\n04-12  Dana\n'''\n",
        );
        let at = |now| kind(&state_at(&birthday, None, now));
        assert_eq!(at("2026-04-04T23:59"), "quiet");
        assert_eq!(at("2026-04-05T00:00"), "upcoming 2026-04-12", "a week ahead");
        assert_eq!(at("2026-04-12T23:00"), "today 2026-04-12");
        assert_eq!(at("2026-04-13T00:00"), "quiet", "lapsed");
    }

    /// A sticky reminder stays overdue after its date until its round is marked — however long ago,
    /// with nothing marked yet — and the next round reminds in its own notice.
    #[test]
    fn a_sticky_reminder_waits_to_be_marked() {
        let bills = topic(
            "bills",
            "recurring = true\nsticky = true\nnotice = \"5 days\"\npending = '''\n* * 1 * *  Rent\n03-01  Car tax\n'''\n",
        );
        let rent = |round: Option<&Entry>, now| {
            kind(&state(&bills.reminders[0], &bills.settings, round, &at(now)))
        };
        assert_eq!(rent(None, "2026-11-06T09:00"), "overdue 2026-11-01");
        assert_eq!(rent(Some(&round("Rent", "2026-11-01")), "2026-11-06T09:00"), "quiet");
        assert_eq!(
            rent(Some(&round("Rent", "2026-11-01")), "2026-11-26T00:00"),
            "upcoming 2026-12-01"
        );
        assert_eq!(
            rent(Some(&round("Rent", "2026-10-01")), "2026-11-06T09:00"),
            "overdue 2026-11-01",
            "last month's mark"
        );
        let car_tax =
            kind(&state(&bills.reminders[1], &bills.settings, None, &at("2026-11-06T09:00")));
        assert_eq!(
            car_tax, "overdue 2026-03-01",
            "written in November, pending since March until marked once"
        );
    }

    /// A chore is pending, whatever the date.
    #[test]
    fn a_chore_is_pending() {
        let chores = topic("housefix", "pending = '''\n-  Fix the door\n'''\n");
        assert_eq!(state_at(&chores, None, "2026-11-06T09:00"), State::Pending);
    }

    /// A reminder timed later today is coming; once its moment is come, it lapses, or — sticky —
    /// is overdue.
    #[test]
    fn a_timed_reminder_turns_at_its_moment() {
        let dentist = topic("appointments", "pending = '''\n2026-11-20 10:30  Dentist\n'''\n");
        assert_eq!(
            kind(&state_at(&dentist, None, "2026-11-20T09:00")),
            "upcoming 2026-11-20T10:30"
        );
        assert_eq!(kind(&state_at(&dentist, None, "2026-11-20T10:30")), "quiet");
        let sticky =
            topic("forms", "sticky = true\npending = '''\n2026-11-20 10:30  Send the form\n'''\n");
        assert_eq!(kind(&state_at(&sticky, None, "2026-11-20T11:00")), "overdue 2026-11-20T10:30");
    }

    /// A round marked ahead of time is the next one not handled: marking twice reaches further.
    #[test]
    fn a_round_marked_early_is_the_next_one() {
        let bills = topic(
            "bills",
            "recurring = true\nsticky = true\npending = '''\n* * 1 * *  Rent\n'''\n",
        );
        let (reminder, settings) = (&bills.reminders[0], &bills.settings);
        let now = at("2026-11-10T09:00");
        let current = round("Rent", "2026-11-01");
        assert_eq!(
            round_to_mark(reminder, settings, None, &now).unwrap().key(),
            "2026-11-01",
            "the overdue one first"
        );
        assert_eq!(
            round_to_mark(reminder, settings, Some(&current), &now).unwrap().key(),
            "2026-12-01"
        );
        let ahead = round("Rent", "2026-12-01");
        assert_eq!(
            round_to_mark(reminder, settings, Some(&ahead), &now).unwrap().key(),
            "2027-01-01"
        );
    }

    /// A yearly cycle's alerts count the years their dates reach, up to the date alerted of — a
    /// birthday ahead says the age it brings — and a reminder written without its year counts none.
    /// A file that isn't a yearly cycle counts no years at all.
    #[test]
    fn a_yearly_cycle_counts_the_years_up_to_its_date() {
        let shelf = |text: &str| {
            let topic = topic("birthday", text);
            let record = Record::of(&topic.path);
            [(topic, record)]
        };
        let rows = "pending = '''\n1990-11-15  Dana\n1955-11-10  Grandpa\n11-15  Noa\n2025-11-12  Baby\n'''\n";
        let now = at("2026-11-10T09:00");
        let years = |shelf: &[(Topic, Record)]| -> Vec<(String, Option<i16>)> {
            due(shelf, &now)
                .listed
                .iter()
                .map(|alert| (alert.reminder.what.clone(), alert.years))
                .collect()
        };
        let birthdays = shelf(&format!("recurring = true\nnotice = \"1 week\"\n{rows}"));
        assert_eq!(
            years(&birthdays),
            [
                ("Dana".into(), Some(36)),
                ("Grandpa".into(), Some(71)),
                ("Noa".into(), None),
                ("Baby".into(), Some(1))
            ]
        );
        let ahead = "recurring = true\nnotice = \"1 week\"\npending = '''\n1990-11-15  Dana\n2027-06-01  Wedding\n'''\n";
        let ahead = shelf(ahead);
        assert!(
            years(&ahead).iter().all(|(_, years)| years.is_none()),
            "a date written ahead: not a yearly cycle"
        );
    }

    /// Sticky reminders and chores are counted per topic; the rest are listed — all by urgency,
    /// most urgent first, then by topic, then in their file's order.
    #[test]
    fn reminders_are_listed_or_counted_in_urgency_s_order() {
        let shelf: Vec<(Topic, Record)> = [
            topic("birthday", "recurring = true\nnotice = \"1 week\"\npending = '''\n11-12  Dana\n11-08  Grandpa\n'''\n"),
            topic("appointments", "urgency = 4\npending = '''\n2026-11-06 18:00  Dentist\n'''\n"),
            topic("bills", "recurring = true\nsticky = true\nurgency = 4\nnotice = \"5 days\"\npending = '''\n* * 1 * *  Rent\n* * 9 * *  Water\n* * 20 * *  Gym\n'''\n"),
            topic("housefix", "urgency = 1\npending = '''\n-  Fix the door\n-  Paint\n'''\n"),
        ]
        .into_iter()
        .map(|topic| {
            let mut record = Record::of(&topic.path);
            if topic.name == "bills" {
                // Written long since: last month's rounds are marked, as they would be by now.
                record.entries.push(round("Water", "2026-10-09"));
                record.entries.push(round("Gym", "2026-10-20"));
            }
            (topic, record)
        })
        .collect();
        let due = due(&shelf, &at("2026-11-06T09:00"));
        let listed: Vec<(&str, &str)> = due
            .listed
            .iter()
            .map(|alert| (alert.topic.name.as_str(), alert.reminder.what.as_str()))
            .collect();
        assert_eq!(
            listed,
            [("appointments", "Dentist"), ("birthday", "Dana"), ("birthday", "Grandpa")]
        );
        let counted: Vec<(&str, usize, usize)> = due
            .counted
            .iter()
            .map(|count| (count.topic.name.as_str(), count.alerts.len(), count.overdue()))
            .collect();
        assert_eq!(
            counted,
            [("bills", 2, 1), ("housefix", 2, 0)],
            "Rent overdue, Water coming; the gym's round marked"
        );
    }
}
