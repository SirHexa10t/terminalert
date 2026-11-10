//! The command line, as a library: all the `terminalert` binary is, and what a program embedding
//! terminalert — bashrs — reuses rather than restating the commands.
//!
//! Each command's arguments are a struct of their own, the folder among them ([`Folder`]), so a
//! program that lifts one command into a command of its own gets every option it takes, and can
//! hand it a folder of its own. [`Command::run`] does what a command says and returns the exit
//! status as a plain number. The commands' names are working names — each is its variant here.

use std::io::IsTerminal;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use jiff::Zoned;

use crate::done::{Entry, How, Record};
use crate::error::{Error, Result};
use crate::render::{self, Colors};
use crate::topic::Topic;
use crate::{due, folder, marking};

/// Reminders for the terminal: what is due in your files of reminders — half calendar, half to-do
/// list.
///
/// `terminalert` alone is `terminalert show`.
#[derive(Parser, Debug)]
#[command(name = "terminalert", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[command(flatten)]
    pub folder: Folder,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Print what is due: the reminders coming up and due today, and how many are pending.
    Show(ShowArgs),
    /// List in full the pending reminders that `show` only counts.
    List(TopicsArgs),
    /// Mark reminders done or skipped: picked in a form, or named.
    Mark(MarkArgs),
    /// Undo a mark: picked in a form, or named.
    Undo(UndoArgs),
    /// Spell out each topic's schedule: every timing in words with its next dates, and every
    /// problem in its file.
    Schedule(TopicsArgs),
}

/// Where the reminder files are.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct Folder {
    /// The folder of reminder files [default: $TERMINALERT_CONFIG_DIR, else
    /// $XDG_CONFIG_HOME/terminalert, else ~/.config/terminalert].
    #[arg(long, value_name = "DIR")]
    pub folder: Option<PathBuf>,
}

impl Folder {
    /// The folder: the one handed over, or the default.
    pub fn path(&self) -> Result<PathBuf> {
        self.folder.clone().map_or_else(folder::default_folder, Ok)
    }
}

/// What `show` takes.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct ShowArgs {
    #[command(flatten)]
    pub folder: Folder,
}

/// What `list` and `schedule` take.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct TopicsArgs {
    /// Only these topics — a file's name without `.toml`; all of them by default.
    pub topics: Vec<String>,
    #[command(flatten)]
    pub folder: Folder,
}

/// What `mark` takes.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkArgs {
    /// The topic — a file's name without `.toml`. Alone, it narrows the form to it.
    pub topic: Option<String>,
    /// The reminder, as its Reminder column says it. Named, there's no form.
    pub reminder: Option<String>,
    /// Mark it skipped, rather than done.
    #[arg(long)]
    pub skipped: bool,
    /// A recurring reminder: take it out of its file for good, rather than mark this round.
    #[arg(long)]
    pub for_good: bool,
    #[command(flatten)]
    pub folder: Folder,
}

/// What `undo` takes.
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct UndoArgs {
    /// The topic — a file's name without `.toml`. Alone, it narrows the form to it.
    pub topic: Option<String>,
    /// The reminder whose latest mark to undo. Named, there's no form.
    pub reminder: Option<String>,
    #[command(flatten)]
    pub folder: Folder,
}

/// What a run's exit status says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Everything went through.
    Clean,
    /// It went through, but a file has problems; they were reported.
    Problems,
    /// It couldn't run at all — a usage error, a folder or file that couldn't be read or written.
    CouldNotRun,
}

impl Exit {
    pub fn code(self) -> u8 {
        match self {
            Exit::Clean => 0,
            Exit::Problems => 1,
            Exit::CouldNotRun => 2,
        }
    }
}

impl Cli {
    /// Does what the command line says; returns the exit status.
    pub fn run(self) -> u8 {
        match self.command {
            None => Command::Show(ShowArgs { folder: self.folder }).run(),
            Some(command) if self.folder == Folder::default() => command.run(),
            Some(_) => {
                eprintln!(
                    "error: options go after the command, as in `terminalert show --folder DIR`"
                );
                Exit::CouldNotRun.code()
            }
        }
    }
}

impl Command {
    /// Does what the command says, now; returns the exit status.
    pub fn run(self) -> u8 {
        let now = Zoned::now();
        let exit = match self {
            Command::Show(args) => show(&args.folder, &now),
            Command::List(args) => list(&args, &now),
            Command::Mark(args) => mark(&args, &now),
            Command::Undo(args) => undo(&args),
            Command::Schedule(args) => schedule(&args, &now),
        };
        exit.code()
    }
}

/// The topics in the folder — only those named, when some are — each with its record.
fn shelf(folder: &Folder, only: &[String]) -> Result<Vec<(Topic, Record)>> {
    let dir = folder.path()?;
    let mut topics = folder::topics(&dir)?;
    if let Some(missing) = only.iter().find(|name| !topics.iter().any(|topic| &topic.name == *name))
    {
        return Err(Error::NoTopic { name: missing.clone(), folder: dir });
    }
    if !only.is_empty() {
        topics.retain(|topic| only.contains(&topic.name));
    }
    Ok(topics
        .into_iter()
        .map(|topic| {
            let record = Record::of(&topic.path);
            (topic, record)
        })
        .collect())
}

fn show(folder: &Folder, now: &Zoned) -> Exit {
    let shelf = match shelf(folder, &[]) {
        Ok(shelf) => shelf,
        Err(err) => return fatal(&err),
    };
    print!("{}", render::alerts(&due::due(&shelf, now), now, colors()));
    report_briefly(&shelf)
}

fn list(args: &TopicsArgs, now: &Zoned) -> Exit {
    let shelf = match shelf(&args.folder, &args.topics) {
        Ok(shelf) => shelf,
        Err(err) => return fatal(&err),
    };
    let due = due::due(&shelf, now);
    if due.counted.is_empty() {
        println!("Nothing pending.");
    }
    print!("{}", render::counted_in_full(&due, now, colors()));
    report_briefly(&shelf)
}

fn schedule(args: &TopicsArgs, now: &Zoned) -> Exit {
    let shelf = match shelf(&args.folder, &args.topics) {
        Ok(shelf) => shelf,
        Err(err) => return fatal(&err),
    };
    let schedules: Vec<String> =
        shelf.iter().map(|(topic, record)| render::schedule(topic, record, now)).collect();
    if schedules.is_empty() {
        println!("No topics yet.");
    }
    print!("{}", schedules.join("\n"));
    let problems = shelf
        .iter()
        .any(|(topic, record)| !topic.problems.is_empty() || record.unreadable().is_some());
    if problems { Exit::Problems } else { Exit::Clean }
}

fn mark(args: &MarkArgs, now: &Zoned) -> Exit {
    let shelf = match shelf(&args.folder, args.topic.as_slice()) {
        Ok(shelf) => shelf,
        Err(err) => return fatal(&err),
    };
    let named = Picked {
        reminders: Vec::new(),
        how: if args.skipped { How::Skipped } else { How::Done },
        for_good: args.for_good,
    };
    let picked = match (&args.topic, &args.reminder) {
        (Some(_), Some(reminder)) => {
            Picked { reminders: vec![(&shelf[0].0, reminder.clone())], ..named }
        }
        _ => match pick_reminders(&shelf, named, now) {
            Ok(Some(picked)) => picked,
            Ok(None) => {
                eprintln!("Nothing marked.");
                return Exit::Clean;
            }
            Err(err) => return fatal(&err),
        },
    };
    let mut exit = Exit::Clean;
    for (topic, what) in picked.reminders {
        let done = topic
            .reminder(&what)
            .ok_or_else(|| Error::NoReminder { topic: topic.name.clone(), reminder: what.clone() })
            .and_then(|reminder| marking::mark(topic, reminder, picked.how, picked.for_good, now));
        match done {
            Ok(entry) => println!("{}", render::marked(&topic.name, &entry)),
            Err(err) => exit = fatal(&err),
        }
    }
    exit
}

fn undo(args: &UndoArgs) -> Exit {
    let shelf = match shelf(&args.folder, args.topic.as_slice()) {
        Ok(shelf) => shelf,
        Err(err) => return fatal(&err),
    };
    let picked: Vec<(&Topic, Entry)> = match (&args.topic, &args.reminder) {
        (Some(_), Some(reminder)) => {
            let (topic, record) = &shelf[0];
            match record.entries.iter().rev().find(|entry| &entry.reminder == reminder) {
                Some(entry) => vec![(topic, entry.clone())],
                None => {
                    return fatal(&Error::NoReminder {
                        topic: topic.name.clone(),
                        reminder: reminder.clone(),
                    });
                }
            }
        }
        _ => match pick_marks(&shelf) {
            Ok(Some(picked)) => picked,
            Ok(None) => {
                eprintln!("Nothing undone.");
                return Exit::Clean;
            }
            Err(err) => return fatal(&err),
        },
    };
    let mut exit = Exit::Clean;
    for (topic, entry) in picked {
        match marking::undo(topic, &entry) {
            Ok(()) => println!("{}", render::undone(&topic.name, &entry)),
            Err(err) => exit = fatal(&err),
        }
    }
    exit
}

/// What to mark, and how.
struct Picked<'a> {
    reminders: Vec<(&'a Topic, String)>,
    how: How,
    for_good: bool,
}

/// The reminders to mark, and how, picked in a form that opens with `given`'s how — `None` when
/// it's left without submitting.
fn pick_reminders<'a>(
    shelf: &'a [(Topic, Record)],
    given: Picked<'a>,
    now: &Zoned,
) -> Result<Option<Picked<'a>>> {
    let mut form = terminal_choice::Form::new()
        .title("Mark reminders")
        .comment("Tick what to mark, choose how, then Submit — Esc leaves everything as it is.");
    for (topic, record) in shelf {
        let choices: Vec<terminal_choice::Choice> = topic
            .reminders
            .iter()
            .map(|reminder| {
                let state =
                    due::state(reminder, &topic.settings, record.round(&reminder.what), now);
                terminal_choice::Choice::named(reminder.what.clone())
                    .note(render::table_when(&state, now))
            })
            .collect();
        if !choices.is_empty() {
            form = form.choices(group(topic), choices);
        }
    }
    form = form
        .radio(HOW, &[How::Done.word(), How::Skipped.word()])
        .checkboxes(RECURRING, &[FOR_GOOD]);
    if !run_form(&mut form)? {
        return Ok(None);
    }
    let how = match form.chosen(HOW) {
        Some(chosen) if chosen == How::Skipped.word() => How::Skipped,
        Some(_) => How::Done,
        None => given.how,
    };
    let for_good = given.for_good || form.checked(RECURRING).contains(&FOR_GOOD);
    let reminders = shelf
        .iter()
        .flat_map(|(topic, _)| {
            form.checked(&group(topic)).into_iter().map(move |what| (topic, what.to_owned()))
        })
        .collect();
    Ok(Some(Picked { reminders, how, for_good }))
}

/// The marks to undo, picked in a form — `None` when it's left without submitting.
fn pick_marks(shelf: &[(Topic, Record)]) -> Result<Option<Vec<(&Topic, Entry)>>> {
    let mut form = terminal_choice::Form::new().title("Undo marks").comment(
        "Tick the marks to undo, latest first, then Submit — Esc leaves everything as it is.",
    );
    for (topic, record) in shelf {
        let labels: Vec<String> = record.entries.iter().rev().map(render::entry_label).collect();
        if !labels.is_empty() {
            let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
            form = form.checkboxes(group(topic), &labels);
        }
    }
    if !run_form(&mut form)? {
        return Ok(None);
    }
    let picked = shelf
        .iter()
        .flat_map(|(topic, record)| {
            let ticked = form.checked(&group(topic));
            record
                .entries
                .iter()
                .filter(move |entry| ticked.contains(&render::entry_label(entry).as_str()))
                .map(move |entry| (topic, entry.clone()))
        })
        .collect();
    Ok(Some(picked))
}

/// A form's group for `topic`: its file's name, which no other group can have.
fn group(topic: &Topic) -> String {
    topic.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The marking form's other groups.
const HOW: &str = "Mark as";
const RECURRING: &str = "Recurring reminders";
const FOR_GOOD: &str = "for good: out of their files";

/// Put `form` on the terminal; whether it was submitted.
fn run_form(form: &mut terminal_choice::Form) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Err(Error::NeedsTerminal);
    }
    let outcome = terminal_choice::run(form).map_err(|source| Error::Form { source })?;
    Ok(outcome == terminal_choice::Outcome::Submitted)
}

/// Each topic's problems, a line each, where the reminders print.
fn report_briefly(shelf: &[(Topic, Record)]) -> Exit {
    let lines: Vec<String> = shelf
        .iter()
        .filter_map(|(topic, record)| render::problems_briefly(topic, record))
        .collect();
    for line in &lines {
        eprintln!("{line}");
    }
    if lines.is_empty() { Exit::Clean } else { Exit::Problems }
}

/// Colours when printing to a terminal — not when `NO_COLOR` asks otherwise.
fn colors() -> Colors {
    if std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() {
        Colors::Ansi
    } else {
        Colors::Plain
    }
}

/// A run that can't go on, said as clap says a usage error: `error: …`, never naming this program
/// — through a program that offers a command as one of its own, the message is that command's.
fn fatal(err: &Error) -> Exit {
    eprintln!("error: {err}");
    Exit::CouldNotRun
}
