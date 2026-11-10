//! Marking a reminder done or skipped, and undoing a mark.
//!
//! A recurring reminder's mark handles its round: the record says so, and the reminder stays in
//! its file. A to-do's reminder — or a recurring one marked for good — leaves its file, and its
//! line goes to the record whole, for undo to put back. Nothing is ever lost on the way: the
//! record is written before the file, and put back as it was if the file can't be written.

use jiff::Zoned;

use crate::done::{Entry, How, Record};
use crate::due;
use crate::error::{Error, Result};
use crate::topic::{Reminder, Topic};

/// Mark `reminder`, of `topic`, as `how` at `now` — for good, where a recurring reminder is to
/// leave its file rather than have its round handled. Returns the record's entry for it.
pub fn mark(
    topic: &Topic,
    reminder: &Reminder,
    how: How,
    for_good: bool,
    now: &Zoned,
) -> Result<Entry> {
    let mut record = Record::of(&topic.path);
    if let Some(err) = record.unreadable() {
        return Err(err);
    }
    let marked = now.strftime("%Y-%m-%dT%H:%M:%S%:z[%Q]").to_string();
    let round = || due::round_to_mark(reminder, &topic.settings, record.round(&reminder.what), now);
    if topic.settings.recurring && !for_good {
        let round = round().ok_or_else(|| Error::NoRound {
            topic: topic.name.clone(),
            reminder: reminder.what.clone(),
        })?;
        let entry = Entry {
            reminder: reminder.what.clone(),
            marked,
            how,
            due: Some(round.key()),
            row: None,
        };
        record.entries.retain(|kept| !(kept.reminder == reminder.what && kept.is_round()));
        record.entries.push(entry.clone());
        record.write()?;
        return Ok(entry);
    }
    let without = topic.without(reminder)?;
    let due = if topic.settings.recurring {
        round()
    } else {
        let (latest, next) = reminder.timing.around(now);
        latest.or(next)
    };
    let entry = Entry {
        reminder: reminder.what.clone(),
        marked,
        how,
        due: due.map(|occurrence| occurrence.key()),
        row: Some(reminder.row.clone()),
    };
    let before = record.entries.clone();
    record.entries.retain(|kept| !(kept.reminder == reminder.what && kept.is_round()));
    record.entries.push(entry.clone());
    record.write()?;
    if let Err(err) = topic.write(&without) {
        record.entries = before;
        let _ = record.write();
        return Err(err);
    }
    Ok(entry)
}

/// Undo `entry`, a mark in `topic`'s record: a round, made pending again — or a reminder taken out,
/// put back at the end of its file's table.
pub fn undo(topic: &Topic, entry: &Entry) -> Result<()> {
    let mut record = Record::of(&topic.path);
    if let Some(err) = record.unreadable() {
        return Err(err);
    }
    let position = record.entries.iter().position(|kept| kept == entry).ok_or_else(|| {
        Error::NoReminder { topic: topic.name.clone(), reminder: entry.reminder.clone() }
    })?;
    if let Some(row) = &entry.row {
        if topic.reminder(&entry.reminder).is_some() {
            return Err(Error::AlreadyThere {
                topic: topic.name.clone(),
                reminder: entry.reminder.clone(),
            });
        }
        topic.write(&topic.with(row)?)?;
    }
    record.entries.remove(position);
    record.write()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::due::State;

    fn at(text: &str) -> Zoned {
        format!("{text}[Europe/London]").parse().unwrap()
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("terminalert_marking_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file(dir: &Path, name: &str, text: &str) -> Topic {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        Topic::read(&path).unwrap()
    }

    /// Marking a recurring reminder handles its round, and the reminder stays; undoing makes the
    /// round pending again.
    #[test]
    fn a_round_is_marked_and_unmarked() {
        let dir = scratch("round");
        let text = "recurring = true\nsticky = true\npending = '''\n* * 1 * *  Rent\n'''\n";
        let bills = file(&dir, "bills.toml", text);
        let now = at("2026-11-06T09:00");
        let state = |topic: &Topic| {
            let record = Record::of(&topic.path);
            due::state(&topic.reminders[0], &topic.settings, record.round("Rent"), &now)
        };
        assert!(matches!(state(&bills), State::Overdue(_)));
        mark(&bills, &bills.reminders[0], How::Done, false, &now).unwrap();
        assert_eq!(
            std::fs::read_to_string(&bills.path).unwrap(),
            text,
            "the reminder stays as it was"
        );
        assert_eq!(state(&bills), State::Quiet);
        let record = Record::of(&bills.path);
        assert_eq!(record.entries.len(), 1);
        assert_eq!(record.entries[0].due.as_deref(), Some("2026-11-01"));
        assert_eq!(record.entries[0].marked, "2026-11-06T09:00:00+00:00[Europe/London]");
        mark(&bills, &bills.reminders[0], How::Skipped, false, &now).unwrap();
        let record = Record::of(&bills.path);
        assert_eq!(record.entries.len(), 1, "one entry a reminder, overwritten each round");
        assert_eq!(
            record.entries[0].due.as_deref(),
            Some("2026-12-01"),
            "the next round, marked early"
        );
        undo(&bills, &record.entries[0]).unwrap();
        assert!(
            matches!(state(&bills), State::Overdue(_)),
            "nothing recorded: pending from its latest date"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Marking a to-do takes it out of its file, keeping everything else; undoing puts it back.
    #[test]
    fn a_to_do_leaves_its_file_and_comes_back() {
        let dir = scratch("todo");
        let text = "# chores\nurgency = 1\n\npending = '''\n# Timing  Reminder\n-  Fix the door   # the hinge\n-  Paint\n'''\n";
        let chores = file(&dir, "housefix.toml", text);
        mark(&chores, &chores.reminders[0], How::Done, false, &at("2026-11-06T09:00")).unwrap();
        let after = std::fs::read_to_string(&chores.path).unwrap();
        assert_eq!(after, text.replace("-  Fix the door   # the hinge\n", ""));
        let record = Record::of(&chores.path);
        assert_eq!(record.entries[0].row.as_deref(), Some("-  Fix the door   # the hinge"));
        assert_eq!(record.entries[0].due, None, "a chore has none");
        let chores = Topic::read(&chores.path).unwrap();
        undo(&chores, &record.entries[0]).unwrap();
        assert_eq!(
            std::fs::read_to_string(&chores.path).unwrap(),
            text.replace("-  Fix the door   # the hinge\n", "")
                .replace("-  Paint\n", "-  Paint\n-  Fix the door   # the hinge\n")
        );
        assert!(Record::of(&chores.path).entries.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A recurring reminder marked for good leaves its file like a to-do's; its round entry goes
    /// with it.
    #[test]
    fn a_recurring_reminder_can_leave_for_good() {
        let dir = scratch("for_good");
        let bills = file(
            &dir,
            "bills.toml",
            "recurring = true\npending = '''\n* * 20 * *  Gym\n* * 1 * *  Rent\n'''\n",
        );
        let now = at("2026-11-06T09:00");
        mark(&bills, &bills.reminders[0], How::Done, false, &now).unwrap();
        mark(&bills, &bills.reminders[0], How::Done, true, &now).unwrap();
        assert_eq!(
            std::fs::read_to_string(&bills.path).unwrap(),
            "recurring = true\npending = '''\n* * 1 * *  Rent\n'''\n"
        );
        let record = Record::of(&bills.path);
        assert_eq!(record.entries.len(), 1, "the round entry is superseded");
        assert_eq!(record.entries[0].row.as_deref(), Some("* * 20 * *  Gym"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Undo won't write a reminder twice; a file changed since it was read isn't written over, and
    /// the record is left as it was.
    #[test]
    fn nothing_is_written_twice_or_lost() {
        let dir = scratch("guards");
        let chores = file(&dir, "housefix.toml", "pending = '''\n-  Fix the door\n'''\n");
        mark(&chores, &chores.reminders[0], How::Done, false, &at("2026-11-06T09:00")).unwrap();
        let entry = Record::of(&chores.path).entries[0].clone();
        std::fs::write(&chores.path, "pending = '''\n-  Fix the door\n'''\n").unwrap();
        let rewritten = Topic::read(&chores.path).unwrap();
        assert!(matches!(undo(&rewritten, &entry), Err(Error::AlreadyThere { .. })));

        let stale = file(&dir, "deadlines.toml", "pending = '''\n2026-12-02  Renew\n'''\n");
        std::fs::write(&stale.path, "pending = '''\n2026-12-02  Renew\n2027-01-31  File\n'''\n")
            .unwrap();
        assert!(matches!(
            mark(&stale, &stale.reminders[0], How::Done, false, &at("2026-11-06T09:00")),
            Err(Error::Changed { .. })
        ));
        assert!(Record::of(&stale.path).entries.is_empty(), "the record rolled back");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
