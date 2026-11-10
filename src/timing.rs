//! A reminder's timing: what its first column says, and when that is.
//!
//! A timing is a date — `2026-11-20` once, `04-12` every year — or, for anything else, cron as
//! crontab writes it: five fields, minute hour day-of-month month weekday. A few short forms are
//! cron underneath (`daily`, `mon`, `mon,thu`, `last`), and a date or a short form may carry a
//! time after it (`2026-11-20 10:30`, `mon 18:00`); cron says its own time, `* *` for the whole
//! day. `-` is no timing: a chore.
//!
//! Where a timing may be written depends on its file: a recurring file's reminders come back, so
//! they take repeating timings — a full date there repeats every year from it — and a to-do
//! file's take full dates, or `-`.

use std::str::FromStr;

use jiff::Zoned;
use jiff::civil::{Date, DateTime, Time};

use crate::words;

/// When a reminder is due.
#[derive(Debug, Clone)]
pub enum Timing {
    /// None: a chore, pending until done.
    None,
    /// Once: a full date in a to-do file — the whole day, or a moment on it.
    Once { date: Date, time: Option<Time> },
    /// Repeating: cron, as the short forms are too.
    Repeating(Box<Repeat>),
}

/// A repeating timing: a cron pattern, and the date it starts from, if any.
#[derive(Debug, Clone)]
pub struct Repeat {
    cron: croner::Cron,
    /// The pattern's five fields, as cron reads them.
    fields: [String; 5],
    /// The full date a recurring file wrote (`1990-04-12`): none of its dates comes before it.
    since: Option<Date>,
}

/// One of a timing's dates: a whole day, or a moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    /// When it starts: the day's first moment, or the moment itself.
    pub start: Zoned,
    /// A whole day, which ends when the day does — or a moment, over once it has come.
    pub all_day: bool,
}

impl Occurrence {
    /// When it is over: the day's end, or the moment itself.
    pub fn end(&self) -> Zoned {
        if !self.all_day {
            return self.start.clone();
        }
        let tomorrow = self.start.date().tomorrow().expect("a date before the end of time");
        day_start(tomorrow, &self.start)
    }

    /// It as a `.done` file records a round: the date, or the date and the time.
    pub fn key(&self) -> String {
        if self.all_day {
            self.start.date().to_string()
        } else {
            self.start.datetime().strftime("%Y-%m-%dT%H:%M").to_string()
        }
    }

    /// Whether a round recorded as `key` (see [`Occurrence::key`]) is this one or a later one.
    pub fn handled_by(&self, key: &str) -> bool {
        parse_key(key).is_some_and(|handled| handled >= self.start.datetime())
    }
}

/// A recorded round's key, as a date and time (a day's key, at its start).
fn parse_key(key: &str) -> Option<DateTime> {
    key.parse::<DateTime>()
        .ok()
        .or_else(|| key.parse::<Date>().ok().map(|date| date.at(0, 0, 0, 0)))
}

impl Timing {
    /// The timing `text` writes, in a recurring file or a to-do file — or what's wrong with it,
    /// said to the writer.
    pub fn parse(text: &str, recurring: bool) -> Result<Timing, String> {
        let text = text.trim();
        if text == words::NO_TIMING {
            return if recurring {
                Err(format!(
                    "`{text}` (no timing) is for to-do files: in a recurring file, a reminder needs a timing to come back by"
                ))
            } else {
                Ok(Timing::None)
            };
        }
        if text.starts_with('@') {
            return Err(format!(
                "cron's `{text}` isn't taken: it's one moment at midnight, not the whole day — write `{}`, or cron's five fields (`* * 1 * *`)",
                words::DAILY
            ));
        }
        let tokens: Vec<&str> = text.split_whitespace().collect();
        let timing = match tokens[..] {
            [_, _, _, _, _] => Timing::Repeating(Box::new(Repeat::cron(text, None)?)),
            [form] => short(form, None, recurring)?,
            [form, time] => short(form, Some(parse_time(time)?), recurring)?,
            _ => return Err(unreadable(text)),
        };
        if !recurring && matches!(timing, Timing::Repeating(_)) {
            return Err(format!(
                "`{text}` repeats, and this is a to-do file: give a full date (2026-04-12), or set `recurring = true`"
            ));
        }
        Ok(timing)
    }

    /// The timing's date `now` is on, or the last before it — and its first after.
    pub fn around(&self, now: &Zoned) -> (Option<Occurrence>, Option<Occurrence>) {
        match self {
            Timing::None => (None, None),
            Timing::Once { date, time } => {
                let once = occurrence_on(*date, *time, now);
                if once.start <= *now { (Some(once), None) } else { (None, Some(once)) }
            }
            Timing::Repeating(repeat) => (repeat.latest(now), repeat.next(now)),
        }
    }

    /// The timing's first `count` dates after `now`.
    pub fn upcoming(&self, now: &Zoned, count: usize) -> Vec<Occurrence> {
        let mut found = Vec::new();
        let mut from = now.clone();
        while found.len() < count {
            let Some(next) = self.around(&from).1 else { break };
            from = next.start.clone();
            found.push(next);
        }
        found
    }

    /// The full date a recurring file wrote it with (`1990-04-12`): the date its dates count their
    /// years from.
    pub fn since(&self) -> Option<Date> {
        match self {
            Timing::Repeating(repeat) => repeat.since(),
            Timing::None | Timing::Once { .. } => None,
        }
    }

    /// Whether it names one day a year: `04-12`, `1990-04-12`, or cron naming one day of one
    /// month (`* * 12 4 *`, `0 9 12 4 *`) — and no weekday, which comes every week.
    pub fn is_once_a_year(&self) -> bool {
        match self {
            Timing::Repeating(repeat) => {
                let [_, _, day, _, weekday] = repeat.fields();
                weekday == "*"
                    && day != "L"
                    && repeat.days().len() == 1
                    && repeat.months().len() == 1
            }
            Timing::None | Timing::Once { .. } => false,
        }
    }
}

/// What can't be read as a timing, said with what can.
fn unreadable(text: &str) -> String {
    format!(
        "can't read the timing `{text}`: write a date (2026-11-20; 04-12 for every year), `{}`, a weekday (`mon`, `mon,thu`), `{}`, cron's five fields, or `{}` for none",
        words::DAILY,
        words::LAST,
        words::NO_TIMING
    )
}

/// A date or a short form, and the time it may carry.
fn short(form: &str, time: Option<Time>, recurring: bool) -> Result<Timing, String> {
    let (minute, hour) = match time {
        Some(time) => (time.minute().to_string(), time.hour().to_string()),
        None => ("*".to_owned(), "*".to_owned()),
    };
    let cron = |day: &str, month: &str, weekday: &str| {
        Repeat::pattern(&format!("{minute} {hour} {day} {month} {weekday}"), None)
            .map(|repeat| Timing::Repeating(Box::new(repeat)))
    };
    if form.eq_ignore_ascii_case(words::DAILY) {
        return cron("*", "*", "*");
    }
    if form.eq_ignore_ascii_case(words::LAST) {
        return cron("L", "*", "*");
    }
    if let Some(days) = weekdays(form) {
        return cron("*", "*", &days);
    }
    let parts: Vec<&str> = form.split('-').collect();
    if !parts.iter().all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())) {
        return Err(unreadable(form));
    }
    let number = |part: &str| part.parse::<i16>().map_err(|_| unreadable(form));
    match parts[..] {
        [year, month, day] if year.len() == 4 => {
            let (year, month, day) = (number(year)?, number(month)?, number(day)?);
            let date = date(year, month, day, form)?;
            if !recurring {
                return Ok(Timing::Once { date, time });
            }
            let pattern = format!("{minute} {hour} {} {} *", date.day(), date.month());
            Ok(Timing::Repeating(Box::new(Repeat::pattern(&pattern, Some(date))?)))
        }
        [month, day] => {
            let (month, day) = (number(month)?, number(day)?);
            // Checked against a leap year, where every day of the year is: `02-29` is a date.
            let date = self::date(2024, month, day, form)?;
            cron(&date.day().to_string(), &date.month().to_string(), "*")
        }
        _ => Err(unreadable(form)),
    }
}

/// `year-month-day`, if that is a date.
fn date(year: i16, month: i16, day: i16, written: &str) -> Result<Date, String> {
    let (Ok(month), Ok(day)) = (i8::try_from(month), i8::try_from(day)) else {
        return Err(not_a_date(written));
    };
    Date::new(year, month, day).map_err(|_| not_a_date(written))
}

/// What a date that isn't one is told.
fn not_a_date(written: &str) -> String {
    format!("`{written}` isn't a date: year-month-day, MONTH BEFORE DAY (April 12th is 04-12)")
}

/// The weekdays `form` names, comma-joined as cron's weekday field writes them — if every name
/// in it is one.
fn weekdays(form: &str) -> Option<String> {
    let names: Vec<String> = form.split(',').map(str::to_ascii_lowercase).collect();
    let known = |name: &String| words::WEEKDAYS.iter().any(|(short, _)| short == name);
    names.iter().all(known).then(|| names.join(","))
}

/// A time as a timing writes it: 24-hour, `H:MM` or `HH:MM`.
fn parse_time(text: &str) -> Result<Time, String> {
    let wrong = || format!("can't read the time `{text}`: write it 24-hour, like 09:30 or 18:00");
    let (hour, minute) = text.split_once(':').ok_or_else(wrong)?;
    let digits = |part: &str, most: usize| {
        (1..=most).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit())
    };
    if !digits(hour, 2) || !digits(minute, 2) || minute.len() != 2 {
        return Err(wrong());
    }
    let (hour, minute) =
        (hour.parse::<i8>().map_err(|_| wrong())?, minute.parse().map_err(|_| wrong())?);
    Time::new(hour, minute, 0, 0).map_err(|_| wrong())
}

/// The characters each of cron's fields may hold, as crontab writes them — names in the month and
/// the weekday only. croner reads more (`L`, `W`, `#`, `?`, a seconds field), none of it crontab's.
fn crontab_field(field: &str, index: usize) -> bool {
    let named = index >= 3;
    field
        .chars()
        .all(|c| c.is_ascii_digit() || "*,-/".contains(c) || (named && c.is_ascii_alphabetic()))
}

impl Repeat {
    /// The cron a file writes — five fields, as crontab writes them — starting from `since`.
    fn cron(text: &str, since: Option<Date>) -> Result<Repeat, String> {
        let fields: Vec<&str> = text.split_whitespace().collect();
        if let Some(i) = (0..fields.len()).find(|&i| !crontab_field(fields[i], i)) {
            return Err(format!(
                "`{}` isn't cron as crontab writes it: digits, month and weekday names, `*`, `,`, `-` and `/` only",
                fields[i]
            ));
        }
        Repeat::pattern(text, since)
    }

    /// A pattern croner reads, starting from `since` — a short form's translation (`last` is
    /// croner's `L`) or the cron a file writes, already checked.
    fn pattern(text: &str, since: Option<Date>) -> Result<Repeat, String> {
        let fields: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
        let fields: [String; 5] = fields.try_into().map_err(|_| unreadable(text))?;
        let text = fields.join(" ");
        let cron = croner::Cron::from_str(&text)
            .map_err(|why| format!("can't read the cron `{text}`: {why}"))?;
        let repeat = Repeat { cron, fields, since };
        let from = since.unwrap_or(Date::constant(2000, 1, 1));
        let reference = from.to_zoned(jiff::tz::TimeZone::UTC).expect("a day in UTC");
        if repeat.cron.find_next_occurrence(&reference, true).is_err() {
            return Err(format!("`{text}` never comes: no date matches it"));
        }
        Ok(repeat)
    }

    /// The pattern's five fields, as cron reads them.
    pub fn fields(&self) -> &[String; 5] {
        &self.fields
    }

    /// The full date it starts from, if a recurring file wrote one.
    pub fn since(&self) -> Option<Date> {
        self.since
    }

    /// Whether each of its dates is a whole day: its time is cron's `* *`.
    pub fn all_day(&self) -> bool {
        self.fields[0] == "*" && self.fields[1] == "*"
    }

    /// The days of the month it names, from 1 (empty for the month's last day, `last`).
    pub fn days(&self) -> Vec<u16> {
        self.cron.pattern.days.values()
    }

    /// The months it names, from 1.
    pub fn months(&self) -> Vec<u16> {
        self.cron.pattern.months.values()
    }

    /// The weekdays it names, Sunday as 0.
    pub fn weekdays(&self) -> Vec<u16> {
        let mut days: Vec<u16> =
            self.cron.pattern.days_of_week.values().into_iter().map(|day| day % 7).collect();
        days.sort_unstable();
        days.dedup();
        days
    }

    /// The hours and minutes of the day it names — its dates' time, when they have one.
    pub fn times(&self) -> (Vec<u16>, Vec<u16>) {
        (self.cron.pattern.hours.values(), self.cron.pattern.minutes.values())
    }

    /// Its date `now` is on, or the last before it.
    fn latest(&self, now: &Zoned) -> Option<Occurrence> {
        let found = if self.all_day() {
            let today = day_start(now.date(), now);
            let day = if self.cron.is_time_matching(&today).unwrap_or(false) {
                Some(today)
            } else {
                self.cron.find_previous_occurrence(&today, false).ok()
            };
            day.map(|day| self.on(&day))
        } else {
            self.cron.find_previous_occurrence(now, true).ok().map(|at| self.on(&at))
        };
        found.filter(|occurrence| self.since.is_none_or(|since| occurrence.start.date() >= since))
    }

    /// Its first date after `now`.
    fn next(&self, now: &Zoned) -> Option<Occurrence> {
        let after = match self.since.map(|since| day_start(since, now)) {
            Some(since) if since > *now => {
                return self.cron.find_next_occurrence(&since, true).ok().map(|at| self.on(&at));
            }
            _ => now,
        };
        if self.all_day() {
            let tomorrow = day_start(after.date().tomorrow().ok()?, after);
            self.cron.find_next_occurrence(&tomorrow, true).ok().map(|day| self.on(&day))
        } else {
            self.cron.find_next_occurrence(after, false).ok().map(|at| self.on(&at))
        }
    }

    /// The occurrence a match at `at` is: its whole day, or the moment.
    fn on(&self, at: &Zoned) -> Occurrence {
        if self.all_day() {
            Occurrence { start: day_start(at.date(), at), all_day: true }
        } else {
            Occurrence { start: at.clone(), all_day: false }
        }
    }
}

/// A once-only timing's date, on its day in `now`'s time zone.
fn occurrence_on(date: Date, time: Option<Time>, now: &Zoned) -> Occurrence {
    match time {
        Some(time) => Occurrence {
            start: date
                .to_datetime(time)
                .to_zoned(now.time_zone().clone())
                .expect("a time on a date"),
            all_day: false,
        },
        None => Occurrence { start: day_start(date, now), all_day: true },
    }
}

/// `date`'s first moment, in `zone_of`'s time zone — midnight, or the first moment after it where
/// a clock change skips midnight.
fn day_start(date: Date, zone_of: &Zoned) -> Zoned {
    date.to_zoned(zone_of.time_zone().clone())
        .and_then(|midnight| midnight.start_of_day())
        .expect("a day has a first moment")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Zoned {
        format!("{text}[Europe/London]").parse().unwrap()
    }

    fn recurring(text: &str) -> Timing {
        Timing::parse(text, true).unwrap_or_else(|why| panic!("{text}: {why}"))
    }

    fn days(occurrences: &[Occurrence]) -> Vec<String> {
        occurrences.iter().map(Occurrence::key).collect()
    }

    /// Every form the template shows is read, in the files it belongs in.
    #[test]
    fn every_form_reads_where_it_belongs() {
        for written in ["2026-11-20", "2026-11-20 10:30", "-"] {
            assert!(Timing::parse(written, false).is_ok(), "to-do: {written}");
        }
        for written in [
            "1990-04-12",
            "04-12",
            "4-12",
            "daily",
            "mon",
            "mon,thu",
            "MON,Thu",
            "last",
            "mon 18:00",
            "* * 1 * *",
            "30 9 1 * *",
            "* * 9 */2 *",
            "* * 9 2-12/2 *",
            "* * 15 1,4,7,10 *",
            "* * * * mon-fri",
            "02-29",
        ] {
            assert!(Timing::parse(written, true).is_ok(), "recurring: {written}");
        }
    }

    /// A timing in the wrong kind of file is told where it goes.
    #[test]
    fn a_timing_in_the_wrong_file_says_where_it_goes() {
        let chore = Timing::parse("-", true).unwrap_err();
        assert!(chore.contains("for to-do files"), "{chore}");
        for repeating in ["04-12", "daily", "mon", "* * 1 * *"] {
            let why = Timing::parse(repeating, false).unwrap_err();
            assert!(why.contains("repeats, and this is a to-do file"), "{repeating}: {why}");
        }
    }

    /// What isn't a timing is told so — with what is.
    #[test]
    fn what_isn_t_a_timing_says_what_is() {
        let cases = [
            ("tomorrow", "can't read the timing"),
            ("04-31", "isn't a date"),
            ("13-01", "isn't a date"),
            ("2026-02-30", "isn't a date"),
            ("mon 25:00", "can't read the time"),
            ("mon 9:5", "can't read the time"),
            ("@daily", "one moment at midnight"),
            ("@monthly", "one moment at midnight"),
            ("* * L * *", "isn't cron as crontab writes it"),
            ("0 * * 9 * *", "can't read the timing"),
            ("* * 31 2 *", "never comes"),
            ("* * 32 * *", "can't read the cron"),
            ("mon,funday", "can't read the timing"),
        ];
        for (written, said) in cases {
            let why = Timing::parse(written, true).unwrap_err();
            assert!(why.contains(said), "{written}: {why}");
        }
    }

    /// A whole-day timing is the day: on it, it's the latest; after it, the next one is ahead.
    #[test]
    fn a_whole_day_timing_is_its_day() {
        let rent = recurring("* * 1 * *");
        let (latest, next) = rent.around(&at("2026-11-01T18:00"));
        assert_eq!(latest.unwrap().key(), "2026-11-01", "the day itself, all of it");
        assert_eq!(next.unwrap().key(), "2026-12-01");
        let (latest, _) = rent.around(&at("2026-11-06T09:00"));
        let latest = latest.unwrap();
        assert!(latest.all_day && latest.end() == at("2026-11-02T00:00"), "over when the day is");
    }

    /// The template's two every-other-month patterns name different months — the difference the
    /// schedule's words exist to show.
    #[test]
    fn every_second_month_starts_where_it_says() {
        let now = at("2026-11-06T09:00");
        assert_eq!(
            days(&recurring("* * 9 */2 *").upcoming(&now, 3)),
            ["2026-11-09", "2027-01-09", "2027-03-09"]
        );
        assert_eq!(
            days(&recurring("* * 9 2-12/2 *").upcoming(&now, 3)),
            ["2026-12-09", "2027-02-09", "2027-04-09"]
        );
        assert_eq!(days(&recurring("* * 9 2,4,6,8,10,12 *").upcoming(&now, 1)), ["2026-12-09"]);
    }

    /// The short forms are the cron they stand for.
    #[test]
    fn the_short_forms_are_cron() {
        let now = at("2026-11-06T09:00"); // a Friday
        assert_eq!(days(&recurring("daily").upcoming(&now, 2)), ["2026-11-07", "2026-11-08"]);
        assert_eq!(days(&recurring("mon,thu").upcoming(&now, 2)), ["2026-11-09", "2026-11-12"]);
        assert_eq!(days(&recurring("last").upcoming(&now, 2)), ["2026-11-30", "2026-12-31"]);
        assert_eq!(days(&recurring("04-12").upcoming(&now, 1)), ["2027-04-12"]);
        assert_eq!(days(&recurring("mon 18:00").upcoming(&now, 1)), ["2026-11-09T18:00"]);
        assert_eq!(days(&recurring("02-29").upcoming(&now, 1)), ["2028-02-29"], "leap days only");
    }

    /// A day-of-month and a weekday both set: a day matching either — cron's own rule.
    #[test]
    fn a_day_and_a_weekday_together_match_either() {
        let now = at("2026-11-06T09:00"); // a Friday
        assert_eq!(
            days(&recurring("* * 13 * fri").upcoming(&now, 3)),
            ["2026-11-13", "2026-11-20", "2026-11-27"]
        );
        assert_eq!(
            days(&recurring("* * 10 * fri").upcoming(&now, 2)),
            ["2026-11-10", "2026-11-13"]
        );
    }

    /// A full date in a recurring file repeats every year from it — and not before it.
    #[test]
    fn a_full_date_repeats_yearly_from_itself() {
        let birthday = recurring("1990-04-12");
        let (latest, next) = birthday.around(&at("2026-11-06T09:00"));
        assert_eq!(
            (latest.unwrap().key(), next.unwrap().key()),
            ("2026-04-12".into(), "2027-04-12".into())
        );
        let wedding = recurring("2027-06-01");
        let (latest, next) = wedding.around(&at("2026-11-06T09:00"));
        assert!(latest.is_none(), "nothing before its first date");
        assert_eq!(next.unwrap().key(), "2027-06-01");
    }

    /// A one-off timing is its day or its moment: ahead of it, next; from it on, latest.
    #[test]
    fn a_one_off_timing_comes_once() {
        let dentist = Timing::parse("2026-11-20 10:30", false).unwrap();
        let (latest, next) = dentist.around(&at("2026-11-20T10:00"));
        assert!(latest.is_none());
        assert_eq!(next.unwrap().key(), "2026-11-20T10:30");
        let (latest, next) = dentist.around(&at("2026-11-20T10:30"));
        assert_eq!(latest.unwrap().key(), "2026-11-20T10:30");
        assert!(next.is_none() && dentist.upcoming(&at("2026-12-01T00:00"), 3).is_empty());
    }

    /// A timing with a time is a moment: the latest is the last one come, inclusive of now.
    #[test]
    fn a_timing_with_a_time_is_a_moment() {
        let lesson = recurring("0 18 * * mon,thu");
        let (latest, next) = lesson.around(&at("2026-11-09T18:00"));
        assert_eq!(latest.unwrap().key(), "2026-11-09T18:00");
        assert_eq!(next.unwrap().key(), "2026-11-12T18:00");
    }

    /// One day a year: a date, without its year or with it, or cron naming one day of one month —
    /// never a weekday, nor more days than one.
    #[test]
    fn once_a_year_is_one_day_of_one_month() {
        for yearly in ["04-12", "1990-04-12", "04-12 09:00", "02-29", "* * 12 4 *", "0 9 12 4 *"] {
            assert!(recurring(yearly).is_once_a_year(), "{yearly}");
        }
        for not in
            ["daily", "mon", "last", "* * 1 * *", "* * 12 4,5 *", "* * 1,2 4 *", "* * 12 4 mon"]
        {
            assert!(!recurring(not).is_once_a_year(), "{not}");
        }
        assert!(
            !Timing::parse("2026-11-20", false).unwrap().is_once_a_year(),
            "once is not yearly"
        );
        assert_eq!(recurring("1990-04-12").since(), Some(jiff::civil::date(1990, 4, 12)));
        assert_eq!(recurring("04-12").since(), None);
    }

    /// A round recorded for one date handles it and every one before it.
    #[test]
    fn a_recorded_round_handles_its_date_and_earlier() {
        let rent = recurring("* * 1 * *");
        let (latest, next) = rent.around(&at("2026-11-06T09:00"));
        let (latest, next) = (latest.unwrap(), next.unwrap());
        assert!(latest.handled_by("2026-11-01") && latest.handled_by("2026-12-01"));
        assert!(!next.handled_by("2026-11-01") && !latest.handled_by("2026-10-01"));
        assert!(!latest.handled_by("garbage"));
    }

    /// Across a clock change, a day is still a day, starting at its own midnight.
    #[test]
    fn a_day_holds_across_a_clock_change() {
        let sunday = recurring("* * * * sun");
        let (latest, _) = sunday.around(&at("2026-10-25T12:00")); // clocks went back that morning
        let latest = latest.unwrap();
        assert_eq!(latest.start, at("2026-10-25T00:00"));
        assert_eq!(latest.end(), at("2026-10-26T00:00"));
    }
}
