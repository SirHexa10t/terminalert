//! Reminders for the terminal — half calendar, half to-do list. The user keeps reminders in files,
//! one topic per file (`bills.toml`): settings on top — whether its reminders recur, how long ahead
//! to remind, whether a reminder stays until it is marked, how urgent the topic is — and a table,
//! a reminder a row: its timing, and what to remind of. terminalert says what is due; a reminder
//! marked is pushed aside, into the topic's `.done` file, never lost.
//!
//! `template.toml` is the files' format reference, and the defaults' source; `REQUIREMENTS.md`
//! holds what is decided and what is still open.
//!
//! The names are working names, made to be renamed — each in one place:
//! - the format's words (the short forms, urgency's words, the files' suffixes): [`words`];
//! - a setting's key: its field in [`settings`];
//! - the output's wording and look: [`render`];
//! - a command's name: its variant in `cli`.
//!
//! The rest: [`folder`] finds the topics, [`topic`] reads and edits one, [`timing`] knows when
//! a timing is, [`due`] what is due, [`done`] keeps the marks and [`marking`] makes and undoes
//! them.

#[cfg(feature = "cli")]
pub mod cli;
pub mod done;
pub mod due;
pub mod error;
pub mod folder;
pub mod marking;
pub mod render;
pub(crate) mod row;
pub mod settings;
pub mod timing;
pub mod topic;
pub mod words;
