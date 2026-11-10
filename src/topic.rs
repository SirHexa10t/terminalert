//! A topic's file: its settings and its table of reminders, read — every problem in it placed on
//! its line, and the rest still read — and edited, when a reminder is taken out of the table or
//! put back.
//!
//! An edit changes the table's lines and nothing else, byte for byte: the table must be between
//! `'''` marks, whose text in the file is exactly the value read; and the file must be as it was
//! read when the edit is written, or the edit is refused rather than lose what changed.

use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

use toml::Spanned;

use crate::error::{Error, Result};
use crate::row;
use crate::settings::{self, Settings, Written};
use crate::timing::Timing;

/// A topic: its file, read.
#[derive(Debug, Clone)]
pub struct Topic {
    /// Its name: its file's, without the suffix — `bills` for `bills.toml`.
    pub name: String,
    /// Its file.
    pub path: PathBuf,
    /// Its settings, the defaults filling in what the file leaves out.
    pub settings: Settings,
    /// Its reminders, in the order the file writes them — every one that could be read.
    pub reminders: Vec<Reminder>,
    /// What is wrong in the file, each where it is.
    pub problems: Vec<Problem>,
    /// The file as read: what an edit starts from, and checks the file against.
    text: String,
    /// Where the table's text is in the file — when it is between `'''` marks.
    table: Option<Range<usize>>,
}

/// A reminder: one row of a topic's table.
#[derive(Debug, Clone)]
pub struct Reminder {
    /// When it is due.
    pub timing: Timing,
    /// What it is — its Reminder column.
    pub what: String,
    /// Its note: what follows its `#`.
    pub note: Option<String>,
    /// Its line in the file, from 1.
    pub line: usize,
    /// Its line as written: what is kept of it when it is taken out, to put back.
    pub row: String,
}

/// Something wrong in a topic's file, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Its line, from 1 — none where the file as a whole is at fault.
    pub line: Option<usize>,
    /// What is wrong, said to the writer.
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl Topic {
    /// The topic whose file is `path`.
    pub fn read(path: &Path) -> Result<Topic> {
        let text = std::fs::read_to_string(path)
            .map_err(|source| Error::Read { path: path.to_owned(), source })?;
        Ok(Topic::parse(path, text))
    }

    /// The topic `text` writes, as the file at `path`.
    pub fn parse(path: &Path, text: String) -> Topic {
        let name =
            path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
        let mut topic = Topic {
            name,
            path: path.to_owned(),
            settings: settings::defaults().clone(),
            reminders: Vec::new(),
            problems: Vec::new(),
            text,
            table: None,
        };
        let written: Written = match toml::from_str(&topic.text) {
            Ok(written) => written,
            Err(why) => {
                let line = why.span().map(|span| line_of(&topic.text, span.start));
                topic.problems.push(Problem { line, message: why.message().trim().to_owned() });
                return topic;
            }
        };
        let (settings, missets) = written.settings(settings::defaults());
        topic.settings = settings;
        for (span, message) in missets {
            topic.problems.push(Problem { line: Some(line_of(&topic.text, span.start)), message });
        }
        if let Some(pending) = &written.pending {
            topic.read_table(pending);
        }
        topic
    }

    /// Read the table's rows into reminders, each row that can't be one into a problem.
    fn read_table(&mut self, pending: &Spanned<String>) {
        let (table, first_line) = locate(&self.text, pending);
        self.table = table;
        for (index, line) in pending.get_ref().lines().enumerate() {
            let number = first_line + index;
            let Some(row) = row::split(line) else { continue };
            let mut problem = |message: String| {
                self.problems.push(Problem { line: Some(number), message });
            };
            let [timing, what] = row.columns[..] else {
                problem(columns_wrong(row.columns.len()));
                continue;
            };
            let timing = match Timing::parse(timing, self.settings.recurring) {
                Ok(timing) => timing,
                Err(why) => {
                    problem(why);
                    continue;
                }
            };
            if let Some(first) = self.reminders.iter().find(|reminder| reminder.what == what) {
                problem(format!(
                    "`{what}` is on line {} too: a reminder can be in its file once",
                    first.line
                ));
                continue;
            }
            self.reminders.push(Reminder {
                timing,
                what: what.to_owned(),
                note: row.note.map(str::to_owned),
                line: number,
                row: line.to_owned(),
            });
        }
    }

    /// The file's text with `reminder`'s line taken out of the table.
    pub fn without(&self, reminder: &Reminder) -> Result<String> {
        let table = self.table.clone().ok_or_else(|| self.not_editable())?;
        let changed = || Error::Changed { path: self.path.clone() };
        // The reminder's line within the table: a table's first or last line may share its line in
        // the file with the `'''` marks.
        let line = line_range(&self.text, reminder.line).ok_or_else(changed)?;
        let row = line.start.max(table.start)..line.end.min(table.end);
        if row.start > row.end
            || self.text[row.clone()].trim_end_matches(['\n', '\r']) != reminder.row
        {
            return Err(changed());
        }
        Ok(format!("{}{}", &self.text[..row.start], &self.text[row.end..]))
    }

    /// The file's text with `row` put back, at the end of the table.
    pub fn with(&self, row: &str) -> Result<String> {
        let table = self.table.clone().ok_or_else(|| self.not_editable())?;
        let content = &self.text[table.clone()];
        let added = if content.is_empty() || content.ends_with('\n') {
            format!("{row}\n")
        } else {
            format!("\n{row}")
        };
        Ok(format!("{}{added}{}", &self.text[..table.end], &self.text[table.end..]))
    }

    /// Write `new` over the file — once it is checked to be as it was read: an edit made to it
    /// since would be lost. Written in place, so a file that is a link stays one.
    pub fn write(&self, new: &str) -> Result<()> {
        let now = std::fs::read_to_string(&self.path)
            .map_err(|source| Error::Read { path: self.path.clone(), source })?;
        if now != self.text {
            return Err(Error::Changed { path: self.path.clone() });
        }
        std::fs::write(&self.path, new)
            .map_err(|source| Error::Write { path: self.path.clone(), source })
    }

    /// Whether a reminder named `what` is in the file.
    pub fn reminder(&self, what: &str) -> Option<&Reminder> {
        self.reminders.iter().find(|reminder| reminder.what == what)
    }

    fn not_editable(&self) -> Error {
        Error::NotEditable { path: self.path.clone() }
    }
}

/// What a row with the wrong number of columns is told.
fn columns_wrong(count: usize) -> String {
    if count == 1 {
        "one column where there are two: put two spaces (or a tab) between the timing and the reminder — `-` first, for a reminder with no timing".to_owned()
    } else {
        format!(
            "{count} columns where there are two, the timing and the reminder: inside a column, keep to single spaces — and start a note with `#`"
        )
    }
}

/// Where the table's text sits in the file — only for a table between `'''` marks, whose text in
/// the file is exactly its value — and the line its first line is on.
fn locate(text: &str, pending: &Spanned<String>) -> (Option<Range<usize>>, usize) {
    let span = pending.span();
    let raw = &text[span.clone()];
    let delimiter = if raw.starts_with("'''") || raw.starts_with("\"\"\"") { 3 } else { 1 };
    let mut start = span.start + delimiter;
    for newline in ["\r\n", "\n"] {
        if delimiter == 3 && text[start..].starts_with(newline) {
            start += newline.len();
            break;
        }
    }
    let literal = raw.starts_with("'''") && span.end - 3 >= start;
    let table = literal
        .then(|| start..span.end - 3)
        .filter(|range| text[range.clone()] == **pending.get_ref());
    (table, line_of(text, start))
}

/// The line `offset` is on, from 1.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].bytes().filter(|&byte| byte == b'\n').count() + 1
}

/// Where line `number` (from 1) is in `text`, its line break included.
fn line_range(text: &str, number: usize) -> Option<Range<usize>> {
    let mut start = 0;
    for _ in 1..number {
        start += text[start..].find('\n')? + 1;
    }
    let end = text[start..].find('\n').map_or(text.len(), |at| start + at + 1);
    (start < text.len()).then_some(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Topic {
        Topic::parse(Path::new("/reminders/bills.toml"), text.to_owned())
    }

    fn problems(topic: &Topic) -> Vec<String> {
        topic.problems.iter().map(ToString::to_string).collect()
    }

    /// The template reads as a topic of its own, with no problem: it doubles as the reader's
    /// test, as sequencer's reference does.
    #[test]
    fn the_template_reads_cleanly() {
        let template = parsed(settings::TEMPLATE);
        assert_eq!(problems(&template), Vec::<String>::new());
        let whats: Vec<&str> =
            template.reminders.iter().map(|reminder| reminder.what.as_str()).collect();
        assert_eq!(whats, ["Dentist", "Dana's school play", "Fix the squeaky door"]);
        assert_eq!(template.reminders[0].note.as_deref(), Some("bring the X-rays"));
    }

    /// A topic is named by its file; its reminders keep the file's order and their lines.
    #[test]
    fn a_topic_is_its_file_s_name_and_rows() {
        let topic = parsed(
            "recurring = true\n\npending = '''\n# Timing  Reminder\n* * 1 * *  Rent\n\nlast  Power  # by the 25th\n'''\n",
        );
        assert_eq!(topic.name, "bills");
        let rows: Vec<(usize, &str)> = topic
            .reminders
            .iter()
            .map(|reminder| (reminder.line, reminder.what.as_str()))
            .collect();
        assert_eq!(rows, [(5, "Rent"), (7, "Power")]);
        assert_eq!(topic.reminders[1].row, "last  Power  # by the 25th");
    }

    /// Every problem is reported on its line, and the rows around it are still read.
    #[test]
    fn problems_are_placed_and_the_rest_still_read() {
        let topic = parsed(concat!(
            "recurring = true\n",
            "urgency = 7\n",
            "pending = '''\n",
            "04-12 Dana\n",
            "* * 1 * *  Rent  monthly\n",
            "-  Fix the door\n",
            "11-03  Grandpa\n",
            "11-04  Grandpa\n",
            "'''\n",
        ));
        assert_eq!(
            problems(&topic),
            [
                "line 2: urgency 7 isn't one: it's 1 (whenever) to 5 (ASAP)",
                "line 4: one column where there are two: put two spaces (or a tab) between the timing and the reminder — `-` first, for a reminder with no timing",
                "line 5: 3 columns where there are two, the timing and the reminder: inside a column, keep to single spaces — and start a note with `#`",
                "line 6: `-` (no timing) is for to-do files: in a recurring file, a reminder needs a timing to come back by",
                "line 8: `Grandpa` is on line 7 too: a reminder can be in its file once",
            ]
        );
        assert_eq!(topic.reminders.len(), 1, "Grandpa, once");
    }

    /// A file that isn't TOML, or writes a key that isn't one, says so — and reads as no reminders.
    #[test]
    fn a_broken_file_says_where() {
        let topic = parsed("recurring = true\nstiky = true\n");
        assert_eq!(topic.problems.len(), 1);
        assert_eq!(topic.problems[0].line, Some(2));
        assert!(topic.problems[0].message.contains("stiky"), "{:?}", topic.problems);
        let topic = parsed("pending = '''\n-  never closed\n");
        assert_eq!(topic.problems.len(), 1, "{:?}", topic.problems);
        assert!(topic.reminders.is_empty());
    }

    /// Taking a reminder out changes its line and nothing else; putting it back adds it at the end.
    #[test]
    fn an_edit_changes_the_table_and_nothing_else() {
        let text = "# my chores\nurgency = 1   # whenever\n\npending = '''\n# Timing  Reminder\n-  Fix the door\n-  Paint   # blue\n'''\n# after\n";
        let topic = parsed(text);
        let out = topic.without(&topic.reminders[0]).unwrap();
        assert_eq!(out, text.replace("-  Fix the door\n", ""));
        assert_eq!(
            parsed(&out).with("-  Fix the door").unwrap(),
            text.replace("-  Fix the door\n", "")
                .replace("# blue\n'''", "# blue\n-  Fix the door\n'''")
        );
        let tight = parsed("pending = '''-  Paint'''\n");
        assert_eq!(tight.with("-  Fix").unwrap(), "pending = '''-  Paint\n-  Fix'''\n");
        assert_eq!(
            tight.without(&tight.reminders[0]).unwrap(),
            "pending = ''''''\n",
            "a one-line table too"
        );
    }

    /// A table between `"""` marks reads, but isn't edited: its text in the file isn't its value.
    #[test]
    fn only_a_literal_table_is_edited() {
        let topic = parsed("pending = \"\"\"\n-  Fix the door\n\"\"\"\n");
        assert_eq!(topic.reminders.len(), 1);
        assert!(matches!(topic.without(&topic.reminders[0]), Err(Error::NotEditable { .. })));
        assert!(matches!(topic.with("-  x"), Err(Error::NotEditable { .. })));
    }

    /// An edit isn't written over a file that changed since it was read.
    #[test]
    fn a_file_changed_meanwhile_is_left_alone() {
        let dir = std::env::temp_dir().join(format!("terminalert_topic_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chores.toml");
        std::fs::write(&path, "pending = '''\n-  Fix the door\n'''\n").unwrap();
        let topic = Topic::read(&path).unwrap();
        let out = topic.without(&topic.reminders[0]).unwrap();
        std::fs::write(&path, "pending = '''\n-  Fix the door\n-  Added meanwhile\n'''\n").unwrap();
        assert!(matches!(topic.write(&out), Err(Error::Changed { .. })));
        assert!(std::fs::read_to_string(&path).unwrap().contains("Added meanwhile"));
        let topic = Topic::read(&path).unwrap();
        topic.write(&topic.without(&topic.reminders[0]).unwrap()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "pending = '''\n-  Added meanwhile\n'''\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
