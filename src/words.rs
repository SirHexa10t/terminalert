//! The words of the files' format that aren't a setting's key — the timings' short forms,
//! urgency's words, the files' suffixes — plus the names the output uses for them, in one place:
//! renaming one is a change here. A setting's key is its field in [`crate::settings`]; the
//! reminders' phrasing is [`crate::render`]'s; a command's name, its variant in `cli`.

/// A topic's file: `bills.toml`.
pub const TOPIC_SUFFIX: &str = "toml";
/// The record beside it of the reminders marked: `bills.done`.
pub const DONE_SUFFIX: &str = "done";
/// The folder of example files beside the topics, never read as reminders.
pub const EXAMPLES_DIR: &str = "examples";

/// The timing of a reminder that has none: a chore.
pub const NO_TIMING: &str = "-";
/// The short form for every day.
pub const DAILY: &str = "daily";
/// The short form for every month's last day.
pub const LAST: &str = "last";
/// The weekdays as a timing writes them — cron's names, Sunday first, as cron numbers them
/// from 0 — and as the output spells them.
pub const WEEKDAYS: [(&str, &str); 7] = [
    ("sun", "Sunday"),
    ("mon", "Monday"),
    ("tue", "Tuesday"),
    ("wed", "Wednesday"),
    ("thu", "Thursday"),
    ("fri", "Friday"),
    ("sat", "Saturday"),
];
/// The months as the output spells them, January first.
pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Urgency's words, for its numbers from 1 up: what a file writes as `urgency = 4` prints as
/// "pressing".
pub const URGENCY: [&str; 5] = ["whenever", "can-wait", "soon", "pressing", "ASAP"];

/// How a reminder was marked: done, or skipped.
pub const DONE: &str = "done";
/// See [`DONE`].
pub const SKIPPED: &str = "skipped";

/// The columns of the alerts' table, in its order — the topic before the reminder, as the one-line
/// alert writes them (`birthday:Dana`) — the files' own column names among them.
pub const COLUMN_WHEN: &str = "When";
/// See [`COLUMN_WHEN`].
pub const COLUMN_TOPIC: &str = "Topic";
/// See [`COLUMN_WHEN`]: the files' "Reminder" column.
pub const COLUMN_REMINDER: &str = "Reminder";
/// See [`COLUMN_WHEN`].
pub const COLUMN_NOTE: &str = "Note";
