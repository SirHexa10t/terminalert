//! What was marked: a topic's `.done` file beside its `.toml` — `bills.done` beside
//! `bills.toml` — terminalert's alone, never the user's to write.
//!
//! A to-do's reminder, marked, moves there whole, out of the topic's file. A recurring reminder's
//! round, marked, is recorded there while the reminder stays: one entry a reminder, found by what
//! it says, and overwritten each round. Every entry says when it was marked, and how: done, or
//! skipped.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::words;

/// How a reminder was marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum How {
    Done,
    Skipped,
}

impl How {
    /// Its word, as the output prints it.
    pub fn word(self) -> &'static str {
        match self {
            How::Done => words::DONE,
            How::Skipped => words::SKIPPED,
        }
    }
}

/// One mark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The reminder marked, as its Reminder column says it. Read under its earlier name too.
    #[serde(alias = "task")]
    pub reminder: String,
    /// When it was marked.
    pub marked: String,
    /// How: done, or skipped.
    #[serde(rename = "as")]
    pub how: How,
    /// The date it was marked for — a recurring reminder's round — as
    /// [`crate::timing::Occurrence::key`] writes it; none for a chore. Read under its earlier name
    /// too.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "deadline")]
    pub due: Option<String>,
    /// The reminder's line as it was written, when the mark took it out of its file: what undoing
    /// it puts back. None for a round, whose reminder stayed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<String>,
}

impl Entry {
    /// Whether it records a round of a reminder that stayed in its file.
    pub fn is_round(&self) -> bool {
        self.row.is_none()
    }
}

/// The `.done` file's shape.
#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default, rename = "entry")]
    entries: Vec<Entry>,
}

/// What a `.done` file opens with.
const HEADER: &str =
    "# What was marked done or skipped — kept by terminalert, and not for editing by hand.\n\n";

/// A topic's record: its `.done` file, read.
#[derive(Debug, Clone)]
pub struct Record {
    path: PathBuf,
    /// Its marks, oldest first.
    pub entries: Vec<Entry>,
    /// Why the file couldn't be read, when it couldn't: such a record is empty, and isn't written
    /// over — what the file holds is still the user's.
    unreadable: Option<String>,
}

impl Record {
    /// The record beside `topic_file` — `bills.done` for `bills.toml` — empty while there is
    /// none.
    pub fn of(topic_file: &Path) -> Record {
        let path = topic_file.with_extension(words::DONE_SUFFIX);
        let read = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str::<File>(&text)
                .map(|file| file.entries)
                .map_err(|why| why.to_string()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(err) => Err(err.to_string()),
        };
        match read {
            Ok(entries) => Record { path, entries, unreadable: None },
            Err(why) => Record { path, entries: Vec::new(), unreadable: Some(why) },
        }
    }

    /// Its file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Why its file couldn't be read, if it couldn't.
    pub fn unreadable(&self) -> Option<Error> {
        let message = self.unreadable.clone()?;
        Some(Error::DoneUnreadable { path: self.path.clone(), message })
    }

    /// The round recorded for the recurring reminder `what`.
    pub fn round(&self, what: &str) -> Option<&Entry> {
        self.entries.iter().rev().find(|entry| entry.reminder == what && entry.is_round())
    }

    /// Write it to its file — never over one it couldn't read.
    pub fn write(&self) -> Result<()> {
        if let Some(err) = self.unreadable() {
            return Err(err);
        }
        let body = toml::to_string(&File { entries: self.entries.clone() })
            .expect("a record's entries are plain strings");
        std::fs::write(&self.path, format!("{HEADER}{body}"))
            .map_err(|source| Error::Write { path: self.path.clone(), source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("terminalert_done_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(reminder: &str, row: Option<&str>) -> Entry {
        Entry {
            reminder: reminder.to_owned(),
            marked: "2026-11-02T10:15:00+00:00[UTC]".to_owned(),
            how: How::Done,
            due: Some("2026-11-01".to_owned()),
            row: row.map(str::to_owned),
        }
    }

    /// A record sits beside its topic, empty until something is marked, and reads back as
    /// written — a row's quotes and marks included.
    #[test]
    fn a_record_is_its_topic_s_done_file() {
        let dir = scratch("roundtrip");
        let mut record = Record::of(&dir.join("bills.toml"));
        assert_eq!(record.path(), dir.join("bills.done"));
        assert!(record.entries.is_empty());
        record.entries.push(entry("Rent", None));
        record
            .entries
            .push(entry("Dana's \"party\"", Some("2026-12-02  Dana's \"party\"  # bring 'cake'")));
        record.write().unwrap();
        let text = std::fs::read_to_string(dir.join("bills.done")).unwrap();
        assert!(text.starts_with("# What was marked"), "{text}");
        assert_eq!(Record::of(&dir.join("bills.toml")).entries, record.entries);
        assert_eq!(record.round("Rent"), Some(&record.entries[0]));
        assert_eq!(record.round("Dana's \"party\""), None, "taken out: not a round");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `.done` file that can't be read reads as empty, says why, and is never written over.
    #[test]
    fn an_unreadable_record_is_left_alone() {
        let dir = scratch("unreadable");
        std::fs::write(dir.join("bills.done"), "not = [toml").unwrap();
        let mut record = Record::of(&dir.join("bills.toml"));
        assert!(record.entries.is_empty());
        assert!(matches!(record.unreadable(), Some(Error::DoneUnreadable { .. })));
        record.entries.push(entry("Rent", None));
        assert!(record.write().is_err());
        assert_eq!(std::fs::read_to_string(dir.join("bills.done")).unwrap(), "not = [toml");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
