//! Everything terminalert prints — the alerts, the counted reminders in full, a topic's schedule in
//! words — its wording and its look, in one place. A first look, to be designed with use.

use std::fmt::Write as _;

use jiff::Zoned;
use jiff::civil::Date;
use jiff::fmt::friendly::{Designator, Spacing, SpanPrinter};

use crate::done::{Entry, Record};
use crate::due::{Alert, Count, Due, State};
use crate::timing::{Occurrence, Repeat, Timing};
use crate::topic::Topic;
use crate::words;

/// Whether the output is coloured — when it goes to a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colors {
    Plain,
    Ansi,
}

/// The colours: what is due today, what is coming, and what is overdue, each its own.
const TODAY: &str = "\x1b[1;33m";
const UPCOMING: &str = "\x1b[36m";
const OVERDUE: &str = "\x1b[1;31m";
const RESET: &str = "\x1b[0m";

impl Colors {
    fn paint(self, color: &str, text: &str) -> String {
        match self {
            Colors::Plain => text.to_owned(),
            Colors::Ansi => format!("{color}{text}{RESET}"),
        }
    }

    fn state(self, state: &State, text: &str) -> String {
        match state {
            State::Today(_) => self.paint(TODAY, text),
            State::Upcoming(_) => self.paint(UPCOMING, text),
            State::Overdue(_) => self.paint(OVERDUE, text),
            State::Quiet | State::Pending => text.to_owned(),
        }
    }
}

/// The alerts: one listed reminder as a sentence, several as a table with headers — then the
/// counted reminders, as one sentence of counts. Nothing at all when nothing is due.
pub fn alerts(due: &Due, now: &Zoned, colors: Colors) -> String {
    let mut out = String::new();
    match due.listed.as_slice() {
        [] => {}
        [one] => {
            let when = colors.state(&one.state, &sentence_when(&one.state, now));
            let note =
                one.reminder.note.as_ref().map(|note| format!(" — {note}")).unwrap_or_default();
            writeln!(out, "{when}: {}:{}{note}", one.topic.name, one.reminder.what).unwrap();
        }
        many => {
            let has_notes = many.iter().any(|alert| alert.reminder.note.is_some());
            let mut rows = vec![Row::header(has_notes)];
            rows.extend(many.iter().map(|alert| Row::of(alert, now, has_notes)));
            out += &table(&rows, colors);
            if !due.counted.is_empty() {
                out.push('\n');
            }
        }
    }
    if !due.counted.is_empty() {
        writeln!(out, "You have {} reminders pending.", counts(&due.counted, colors)).unwrap();
    }
    out
}

/// "2 bills (1 overdue), 5 housefix and 3 organizing".
fn counts(counted: &[Count], colors: Colors) -> String {
    let parts: Vec<String> = counted
        .iter()
        .map(|count| {
            let overdue = count.overdue();
            let overdue = if overdue > 0 {
                format!(" ({})", colors.paint(OVERDUE, &format!("{overdue} overdue")))
            } else {
                String::new()
            };
            format!("{} {}{overdue}", count.alerts.len(), count.topic.name)
        })
        .collect();
    list(&parts)
}

/// A reminders table's row: its cells, the state colouring the first.
struct Row {
    cells: Vec<String>,
    state: State,
}

impl Row {
    fn header(has_notes: bool) -> Row {
        // The topic before the reminder, as the one-line alert writes them: `birthday:Dana`.
        let mut cells = [words::COLUMN_WHEN, words::COLUMN_TOPIC, words::COLUMN_REMINDER]
            .map(String::from)
            .to_vec();
        if has_notes {
            cells.push(words::COLUMN_NOTE.to_owned());
        }
        Row { cells, state: State::Quiet }
    }

    fn of(alert: &Alert, now: &Zoned, has_notes: bool) -> Row {
        let mut cells = vec![
            table_when(&alert.state, now),
            alert.topic.name.clone(),
            alert.reminder.what.clone(),
        ];
        if has_notes {
            cells.push(alert.reminder.note.clone().unwrap_or_default());
        }
        Row { cells, state: alert.state.clone() }
    }
}

/// `rows` as columns two spaces apart, each as wide as its widest cell; the first column coloured
/// by its row's state.
fn table(rows: &[Row], colors: Colors) -> String {
    let columns = rows.iter().map(|row| row.cells.len()).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.cells.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for row in rows {
        let cells: Vec<String> = row
            .cells
            .iter()
            .zip(&widths)
            .enumerate()
            .map(|(column, (cell, width))| {
                let padded = format!("{cell:<width$}");
                if column == 0 { colors.state(&row.state, &padded) } else { padded }
            })
            .collect();
        writeln!(out, "{}", cells.join("  ").trim_end()).unwrap();
    }
    out
}

/// A listed reminder's time, as a sentence opens with it: "Upcoming in 3 days", "Due today".
fn sentence_when(state: &State, now: &Zoned) -> String {
    match state {
        State::Today(_) => "Due today".to_owned(),
        State::Upcoming(at) => match days_until(at, now) {
            0 => format!("Due today{}", at_time(at)),
            1 => format!("Upcoming in 1 day{}", at_time(at)),
            days => format!("Upcoming in {days} days{}", at_time(at)),
        },
        State::Overdue(at) => format!("Overdue since {}", short_date(at.start.date())),
        State::Quiet | State::Pending => "Pending".to_owned(),
    }
}

/// A reminder's time, as a table's When column says it: "in 3 days", "today", "overdue since …".
pub(crate) fn table_when(state: &State, now: &Zoned) -> String {
    match state {
        State::Today(_) => "today".to_owned(),
        State::Upcoming(at) => match days_until(at, now) {
            0 => format!("today{}", at_time(at)),
            1 => format!("in 1 day{}", at_time(at)),
            days => format!("in {days} days{}", at_time(at)),
        },
        State::Overdue(at) => format!("overdue since {}", short_date(at.start.date())),
        State::Quiet | State::Pending => words::NO_TIMING.to_owned(),
    }
}

/// " at 18:00" for a moment; nothing for a whole day.
fn at_time(at: &Occurrence) -> String {
    if at.all_day { String::new() } else { format!(" at {}", at.start.strftime("%H:%M")) }
}

/// Days from `now`'s date to the occurrence's.
fn days_until(at: &Occurrence, now: &Zoned) -> i32 {
    now.date().until(at.start.date()).map(|span| span.get_days()).unwrap_or(0)
}

/// The counted reminders in full, topic by topic: what the alerts only count.
pub fn counted_in_full(due: &Due, now: &Zoned, colors: Colors) -> String {
    let mut out = String::new();
    for count in &due.counted {
        let overdue = count.overdue();
        let overdue = if overdue > 0 { format!(", {overdue} overdue") } else { String::new() };
        writeln!(out, "{} — {} pending{overdue}", count.topic.name, count.alerts.len()).unwrap();
        let has_notes = count.alerts.iter().any(|alert| alert.reminder.note.is_some());
        let rows: Vec<Row> = count
            .alerts
            .iter()
            .map(|alert| {
                let mut cells = vec![
                    format!("  {}", table_when(&alert.state, now)),
                    alert.reminder.what.clone(),
                ];
                if has_notes {
                    cells.push(
                        alert
                            .reminder
                            .note
                            .as_ref()
                            .map(|note| format!("# {note}"))
                            .unwrap_or_default(),
                    );
                }
                Row { cells, state: alert.state.clone() }
            })
            .collect();
        out += &table(&rows, colors);
    }
    out
}

/// A topic's schedule: its settings, each reminder's timing in words with its next dates, and
/// every problem in its file.
pub fn schedule(topic: &Topic, record: &Record, now: &Zoned) -> String {
    let settings = &topic.settings;
    let mut out = String::new();
    writeln!(
        out,
        "{} — {}, {}, notice {}, urgency {} ({})",
        topic.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
        if settings.recurring { "recurring" } else { "a to-do list" },
        if settings.sticky { "sticky" } else { "not sticky" },
        SpanPrinter::new()
            .designator(Designator::Verbose)
            .spacing(Spacing::BetweenUnitsAndDesignators)
            .span_to_string(&settings.notice),
        settings.urgency.level(),
        settings.urgency.word()
    )
    .unwrap();
    let width =
        topic.reminders.iter().map(|reminder| reminder.what.chars().count()).max().unwrap_or(0);
    for reminder in &topic.reminders {
        writeln!(out, "  {:<width$}  {}", reminder.what, describe(&reminder.timing)).unwrap();
        let next: Vec<String> = reminder
            .timing
            .upcoming(now, 3)
            .iter()
            .map(|at| short_date(at.start.date()) + &at_time(at))
            .collect();
        if !next.is_empty() {
            writeln!(out, "  {:<width$}  next: {}", "", next.join(", ")).unwrap();
        }
    }
    if let Some(err) = record.unreadable() {
        writeln!(out, "  {err}").unwrap();
    }
    for problem in &topic.problems {
        writeln!(out, "  {problem}").unwrap();
    }
    out
}

/// A topic's problems, briefly — its first, and how many more: a line where the reminders are
/// printed, which the schedule tells in full.
pub fn problems_briefly(topic: &Topic, record: &Record) -> Option<String> {
    let file =
        topic.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some(err) = record.unreadable() {
        return Some(format!("reminders: {err}"));
    }
    let first = topic.problems.first()?;
    let more = match topic.problems.len() - 1 {
        0 => String::new(),
        1 => " — and 1 more problem".to_owned(),
        more => format!(" — and {more} more problems"),
    };
    Some(format!("reminders: {file}, {first}{more}"))
}

/// What a mark did: "Marked bills:Rent done — the round of Sun 1 Nov 2026."
pub fn marked(topic: &str, entry: &Entry) -> String {
    let round = match (&entry.row, &entry.due) {
        (None, Some(key)) => format!(" — the round of {}", key_words(key)),
        (Some(_), _) => " — taken out of its file, kept in its record".to_owned(),
        (None, None) => String::new(),
    };
    format!("Marked {topic}:{} {}{round}.", entry.reminder, entry.how.word())
}

/// What an undo did: "Undid bills:Rent, done on Fri 6 Nov 2026."
pub fn undone(topic: &str, entry: &Entry) -> String {
    let back = if entry.row.is_some() { " — back in its file" } else { "" };
    format!(
        "Undid {topic}:{}, {} {}{back}.",
        entry.reminder,
        entry.how.word(),
        marked_words(&entry.marked)
    )
}

/// A mark in an undo form: "Rent — done Fri 6 Nov 2026 09:00, the round of Sun 1 Nov 2026".
pub fn entry_label(entry: &Entry) -> String {
    let round = match (&entry.row, &entry.due) {
        (None, Some(key)) => format!(", the round of {}", key_words(key)),
        _ => String::new(),
    };
    format!("{} — {} {}{round}", entry.reminder, entry.how.word(), marked_words(&entry.marked))
}

/// When a mark was made, in words: "on Fri 6 Nov 2026 at 09:00".
fn marked_words(marked: &str) -> String {
    match marked.parse::<Zoned>() {
        Ok(at) => format!("on {} at {}", short_date(at.date()), at.strftime("%H:%M")),
        Err(_) => marked.to_owned(),
    }
}

/// A recorded round's key in words: "Sun 1 Nov 2026", "Mon 9 Nov 2026 at 18:00".
fn key_words(key: &str) -> String {
    if let Ok(at) = key.parse::<jiff::civil::DateTime>() {
        return format!("{} at {}", short_date(at.date()), at.strftime("%H:%M"));
    }
    key.parse::<Date>().map(short_date).unwrap_or_else(|_| key.to_owned())
}

/// A timing in words.
pub fn describe(timing: &Timing) -> String {
    match timing {
        Timing::None => "no timing: a chore, pending until done".to_owned(),
        Timing::Once { date, time } => {
            let time =
                time.map(|time| format!(" at {}", time.strftime("%H:%M"))).unwrap_or_default();
            format!("once, on {}{time}", long_date(*date))
        }
        Timing::Repeating(repeat) => repeating(repeat),
    }
}

/// A repeating timing in words: its days, its months listed out — so `*/2` and `2-12/2` say
/// which months they are — its time, and the date it starts from.
fn repeating(repeat: &Repeat) -> String {
    let [_, _, day, _, weekday] = repeat.fields();
    let months = repeat.months();
    let every_month = months.len() == 12;
    let month_names = list(
        &months
            .iter()
            .map(|&month| words::MONTHS[usize::from(month) - 1].to_owned())
            .collect::<Vec<_>>(),
    );
    let days = (day != "*").then(|| {
        if day == "L" {
            "the last day".to_owned()
        } else {
            format!(
                "the {}",
                list(&repeat.days().iter().map(|&day| ordinal(day)).collect::<Vec<_>>())
            )
        }
    });
    let weekdays = (weekday != "*").then(|| {
        let names: Vec<String> = repeat
            .weekdays()
            .iter()
            .map(|&day| words::WEEKDAYS[usize::from(day)].1.to_owned())
            .collect();
        format!("every {}", list(&names))
    });
    let of_months = |days: &str| {
        if every_month {
            format!("{days} of every month")
        } else {
            format!("{days} of {month_names}")
        }
    };
    let dates = match (days, weekdays) {
        (None, None) if every_month => "every day".to_owned(),
        (None, None) => format!("every day of {month_names}"),
        (Some(days), None) if months.len() == 1 && repeat.days().len() == 1 => {
            format!("every year on {days} of {month_names}")
        }
        (Some(days), None) => of_months(&days),
        (None, Some(weekdays)) if every_month => weekdays,
        (None, Some(weekdays)) => format!("{weekdays}, in {month_names}"),
        (Some(days), Some(weekdays)) => format!("{}, and {weekdays}", of_months(&days)),
    };
    let since =
        repeat.since().map(|since| format!(", since {}", long_date(since))).unwrap_or_default();
    format!("{dates}{}{since}", times(repeat))
}

/// A repeating timing's time, in words: nothing for a whole day.
fn times(repeat: &Repeat) -> String {
    if repeat.all_day() {
        return String::new();
    }
    let (hours, minutes) = repeat.times();
    if hours.len() * minutes.len() > 4 {
        let [minute, hour, ..] = repeat.fields();
        return format!(", at the times cron writes as `{minute} {hour}`");
    }
    let times: Vec<String> = hours
        .iter()
        .flat_map(|hour| minutes.iter().map(move |minute| format!("{hour:02}:{minute:02}")))
        .collect();
    format!(", at {}", list(&times))
}

/// `1st`, `2nd`, `3rd`, `4th` … `11th` … `21st`.
fn ordinal(number: u16) -> String {
    let suffix = match (number % 10, number % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{number}{suffix}")
}

/// "a", "a and b", "a, b and c".
fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// "Friday 20 November 2026".
fn long_date(date: Date) -> String {
    let weekday =
        words::WEEKDAYS[usize::from(date.weekday().to_sunday_zero_offset().unsigned_abs())].1;
    format!(
        "{weekday} {} {} {}",
        date.day(),
        words::MONTHS[usize::from(date.month().unsigned_abs()) - 1],
        date.year()
    )
}

/// "Fri 20 Nov 2026".
fn short_date(date: Date) -> String {
    let weekday =
        words::WEEKDAYS[usize::from(date.weekday().to_sunday_zero_offset().unsigned_abs())].1;
    let month = words::MONTHS[usize::from(date.month().unsigned_abs()) - 1];
    format!("{} {} {} {}", &weekday[..3], date.day(), &month[..3], date.year())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::due;

    fn at(text: &str) -> Zoned {
        format!("{text}[Europe/London]").parse().unwrap()
    }

    fn words(timing: &str) -> String {
        describe(&Timing::parse(timing, true).unwrap())
    }

    /// Each timing says when it is in words — every-other-month ones naming their months, so
    /// the two the template shows can't be told apart wrongly.
    #[test]
    fn timings_in_words() {
        assert_eq!(
            words("* * 9 */2 *"),
            "the 9th of January, March, May, July, September and November"
        );
        assert_eq!(
            words("* * 9 2-12/2 *"),
            "the 9th of February, April, June, August, October and December"
        );
        assert_eq!(words("* * 1 * *"), "the 1st of every month");
        assert_eq!(words("30 9 1,15 * *"), "the 1st and 15th of every month, at 09:30");
        assert_eq!(words("last"), "the last day of every month");
        assert_eq!(words("04-12"), "every year on the 12th of April");
        assert_eq!(
            words("1990-04-12"),
            "every year on the 12th of April, since Thursday 12 April 1990"
        );
        assert_eq!(words("daily"), "every day");
        assert_eq!(words("mon,thu 18:00"), "every Monday and Thursday, at 18:00");
        assert_eq!(
            words("* * * * mon-fri"),
            "every Monday, Tuesday, Wednesday, Thursday and Friday"
        );
        assert_eq!(words("* * 13 * fri"), "the 13th of every month, and every Friday");
        assert_eq!(words("0 9,18 * * *"), "every day, at 09:00 and 18:00");
        assert_eq!(words("*/15 * * * *"), "every day, at the times cron writes as `*/15 *`");
        assert_eq!(
            describe(&Timing::parse("2026-11-20 10:30", false).unwrap()),
            "once, on Friday 20 November 2026 at 10:30"
        );
        assert_eq!(
            describe(&Timing::parse("-", false).unwrap()),
            "no timing: a chore, pending until done"
        );
    }

    /// One listed reminder is a sentence; several, a table with headers; the counts follow.
    #[test]
    fn alerts_read_as_a_sentence_or_a_table_then_counts() {
        let topic = |name: &str, text: &str| {
            let topic = Topic::parse(Path::new(&format!("/r/{name}.toml")), text.to_owned());
            let record = Record::of(&topic.path);
            (topic, record)
        };
        let now = at("2026-11-06T09:00");
        let birthday = topic(
            "birthday",
            "recurring = true\nnotice = \"1 week\"\npending = '''\n11-07  Dana\n'''\n",
        );
        let one = [birthday.clone()];
        assert_eq!(
            alerts(&due::due(&one, &now), &now, Colors::Plain),
            "Upcoming in 1 day: birthday:Dana\n"
        );
        let shelf = [
            birthday,
            topic(
                "appointments",
                "pending = '''\n2026-11-06 18:00  Dentist  # bring the X-rays\n'''\n",
            ),
            topic("housefix", "pending = '''\n-  Fix the door\n-  Paint\n'''\n"),
            topic(
                "bills",
                "recurring = true\nsticky = true\npending = '''\n* * 1 * *  Rent\n'''\n",
            ),
        ];
        assert_eq!(
            alerts(&due::due(&shelf, &now), &now, Colors::Plain),
            concat!(
                "When            Topic         Reminder  Note\n",
                "today at 18:00  appointments  Dentist   bring the X-rays\n",
                "in 1 day        birthday      Dana\n",
                "\n",
                "You have 1 bills (1 overdue) and 2 housefix reminders pending.\n",
            )
        );
        assert_eq!(
            alerts(&due::due(&[], &now), &now, Colors::Plain),
            "",
            "nothing due: nothing said"
        );
    }

    /// Coloured, the state's colour wraps its words — and nothing else changes.
    #[test]
    fn colour_marks_the_state_only() {
        let (topic, record) = {
            let topic = Topic::parse(
                Path::new("/r/bills.toml"),
                "recurring = true\nsticky = true\npending = '''\n* * 1 * *  Rent\n'''\n".to_owned(),
            );
            let record = Record::of(&topic.path);
            (topic, record)
        };
        let now = at("2026-11-06T09:00");
        let shelf = [(topic, record)];
        let colored = alerts(&due::due(&shelf, &now), &now, Colors::Ansi);
        assert_eq!(colored, "You have 1 bills (\x1b[1;31m1 overdue\x1b[0m) reminders pending.\n");
    }

    /// A file's problems are told briefly where the reminders print: the first, and a count.
    #[test]
    fn problems_are_told_briefly() {
        let topic = Topic::parse(
            Path::new("/r/bills.toml"),
            "urgency = 9\npending = '''\nmon  x\n-  y\n'''\n".to_owned(),
        );
        let record = Record::of(&topic.path);
        assert_eq!(
            problems_briefly(&topic, &record).unwrap(),
            "reminders: bills.toml, line 1: urgency 9 isn't one: it's 1 (whenever) to 5 (ASAP) — and 1 more problem"
        );
    }

    /// Ordinals and lists read as English writes them.
    #[test]
    fn ordinals_and_lists() {
        let ordinals: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 31].map(ordinal).to_vec();
        assert_eq!(
            ordinals,
            ["1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "23rd", "31st"]
        );
        let items =
            |n: usize| ["a", "b", "c"][..n].iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            [list(&items(1)), list(&items(2)), list(&items(3))],
            ["a", "a and b", "a, b and c"]
        );
    }
}
