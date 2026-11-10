//! The crate's error type: what stops an operation — a file that can't be read or written, a
//! reminder that isn't there. What is wrong INSIDE a topic's file isn't one: it is a
//! [`crate::topic::Problem`], kept with the topic, so one bad line doesn't hide the rest.

use std::io;
use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("can't read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },

    #[error("can't write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },

    #[error("{} changed while this ran, so nothing was written to it — try again", path.display())]
    Changed { path: PathBuf },

    #[error(
        "{}'s table isn't between ''' marks, so it can't be edited here — write it as pending = '''…'''",
        path.display()
    )]
    NotEditable { path: PathBuf },

    #[error("can't read {} (terminalert keeps it; it isn't for editing): {message}", path.display())]
    DoneUnreadable { path: PathBuf, message: String },

    #[error("no folder of reminders: set TERMINALERT_CONFIG_DIR, or HOME")]
    NoFolder,

    #[error("no topic {name}: no {name}.{} in {}", crate::words::TOPIC_SUFFIX, folder.display())]
    NoTopic { name: String, folder: PathBuf },

    #[error("no reminder {reminder:?} in {topic}")]
    NoReminder { topic: String, reminder: String },

    #[error("{reminder:?} has no date left in {topic} to mark")]
    NoRound { topic: String, reminder: String },

    #[error("{reminder:?} is in {topic} already — undoing would write it twice")]
    AlreadyThere { topic: String, reminder: String },

    #[error("picking needs a terminal — or name the TOPIC and the REMINDER")]
    NeedsTerminal,

    #[error("the form failed: {source}")]
    Form { source: io::Error },
}
