//! The binary, driven as a user drives it: a folder of files, the commands, what they print and
//! what they leave on disk. Dates are written relative to today, so the tests hold any day.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A scratch folder of reminder files, removed when dropped.
struct Folder {
    dir: PathBuf,
}

impl Folder {
    fn new(tag: &str) -> Folder {
        let dir =
            std::env::temp_dir().join(format!("terminalert_cli_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Folder { dir }
    }

    fn write(&self, name: &str, text: &str) -> &Folder {
        std::fs::write(self.dir.join(name), text).unwrap();
        self
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.join(name)).unwrap()
    }

    /// `terminalert ARGS --folder <this>`.
    fn run(&self, args: &[&str]) -> Output {
        terminalert(args, Some(&self.dir), None)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// `terminalert ARGS`, handed `folder` with `--folder`, or `config_dir` as
/// `TERMINALERT_CONFIG_DIR` — never the real user's.
fn terminalert(args: &[&str], folder: Option<&Path>, config_dir: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_terminalert"));
    command
        .args(args)
        .env_remove("TERMINALERT_CONFIG_DIR")
        .env("XDG_CONFIG_HOME", "/nonexistent")
        .env("NO_COLOR", "1");
    if let Some(folder) = folder {
        command.arg("--folder").arg(folder);
    }
    if let Some(dir) = config_dir {
        command.env("TERMINALERT_CONFIG_DIR", dir);
    }
    command.output().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Today plus `days`, as a timing writes a yearly date: `MM-DD`.
fn month_day(days: i64) -> String {
    let date = jiff::Zoned::now().date().checked_add(jiff::Span::new().days(days)).unwrap();
    format!("{:02}-{:02}", date.month(), date.day())
}

/// Today plus `days`, as a timing writes a full date.
fn full_date(days: i64) -> String {
    jiff::Zoned::now().date().checked_add(jiff::Span::new().days(days)).unwrap().to_string()
}

/// Nothing due prints nothing; what is due prints as alerts — a listed reminder as a sentence, the
/// chores as a count.
#[test]
fn show_says_what_is_due_and_nothing_else() {
    let folder = Folder::new("show");
    let quiet = folder.run(&["show"]);
    assert_eq!(
        (stdout(&quiet), stderr(&quiet), quiet.status.code()),
        (String::new(), String::new(), Some(0))
    );
    folder.write(
        "birthday.toml",
        &format!(
            "recurring = true\nnotice = \"1 week\"\npending = '''\n{}  Dana\n'''\n",
            month_day(1)
        ),
    );
    folder.write("housefix.toml", "pending = '''\n-  Fix the door\n'''\n");
    let shown = folder.run(&["show"]);
    assert_eq!(
        stdout(&shown),
        "Upcoming in 1 day: birthday:Dana\nYou have 1 housefix reminders pending.\n"
    );
    assert_eq!(shown.status.code(), Some(0));
    assert_eq!(stdout(&folder.run(&[])), stdout(&shown), "terminalert alone is show");
}

/// A broken file is told briefly, on stderr, and the rest still remind.
#[test]
fn show_tells_a_broken_file_briefly() {
    let folder = Folder::new("broken");
    folder.write("housefix.toml", "pending = '''\n-  Fix the door\n'''\n");
    folder.write("bills.toml", "recurring = true\npending = '''\n-  Rent\n'''\n");
    let shown = folder.run(&["show"]);
    assert_eq!(stdout(&shown), "You have 1 housefix reminders pending.\n");
    assert_eq!(
        stderr(&shown),
        "reminders: bills.toml, line 3: `-` (no timing) is for to-do files: in a recurring file, a reminder needs a timing to come back by\n"
    );
    assert_eq!(shown.status.code(), Some(1));
}

/// The schedule spells every timing out, with its next dates and every problem in full.
#[test]
fn schedule_spells_timings_out() {
    let folder = Folder::new("schedule");
    folder.write("bills.toml", "recurring = true\nsticky = true\nurgency = 4\npending = '''\n* * 9 2-12/2 *  Water\n* * 32 * *  Broken\n'''\n");
    let schedule = folder.run(&["schedule", "bills"]);
    let said = stdout(&schedule);
    assert!(
        said.starts_with("bills.toml — recurring, sticky, notice 1 day, urgency 4 (pressing)\n"),
        "{said}"
    );
    assert!(
        said.contains("  Water  the 9th of February, April, June, August, October and December\n"),
        "{said}"
    );
    assert!(said.contains("next: "), "{said}");
    assert!(said.contains("  line 6: can't read the cron `* * 32 * *`"), "{said}");
    assert_eq!(schedule.status.code(), Some(1));
}

/// A reminder named is marked without a form: a to-do leaves its file for its record, and undoing
/// puts it back.
#[test]
fn a_named_reminder_is_marked_and_unmarked() {
    let folder = Folder::new("mark");
    let text = "# my chores\npending = '''\n-  Fix the door   # the hinge\n-  Paint\n'''\n";
    folder.write("housefix.toml", text);
    let marked = folder.run(&["mark", "housefix", "Fix the door"]);
    assert_eq!(
        stdout(&marked),
        "Marked housefix:Fix the door done — taken out of its file, kept in its record.\n"
    );
    assert_eq!(folder.read("housefix.toml"), "# my chores\npending = '''\n-  Paint\n'''\n");
    assert!(folder.read("housefix.done").contains("row = \"-  Fix the door   # the hinge\""));
    let undone = folder.run(&["undo", "housefix", "Fix the door"]);
    assert!(
        stdout(&undone).starts_with("Undid housefix:Fix the door, done on "),
        "{}",
        stdout(&undone)
    );
    assert_eq!(
        folder.read("housefix.toml"),
        "# my chores\npending = '''\n-  Paint\n-  Fix the door   # the hinge\n'''\n"
    );
}

/// A recurring reminder's round is marked, skipped or done, and the reminder stays in its file.
#[test]
fn a_round_is_marked_and_the_reminder_stays() {
    let folder = Folder::new("round");
    let text = format!(
        "recurring = true\nsticky = true\npending = '''\n{}  Insurance\n'''\n",
        month_day(-3)
    );
    folder.write("bills.toml", &text);
    assert!(
        stdout(&folder.run(&["list"])).contains("overdue since"),
        "pending from its latest date"
    );
    let marked = folder.run(&["mark", "bills", "Insurance", "--skipped"]);
    assert!(
        stdout(&marked).starts_with("Marked bills:Insurance skipped — the round of "),
        "{}",
        stdout(&marked)
    );
    assert_eq!(folder.read("bills.toml"), text);
    assert_eq!(stdout(&folder.run(&["list"])), "Nothing pending.\n");
}

/// Picking needs a terminal; without one, it says what to do instead.
#[test]
fn picking_without_a_terminal_says_to_name_the_reminder() {
    let folder = Folder::new("picking");
    folder.write("housefix.toml", "pending = '''\n-  Fix the door\n'''\n");
    let picked = folder.run(&["mark"]);
    assert_eq!(
        stderr(&picked),
        "error: picking needs a terminal — or name the TOPIC and the REMINDER\n"
    );
    assert_eq!(picked.status.code(), Some(2));
}

/// What can't run says why, as an error, with exit status 2.
#[test]
fn what_can_t_run_says_why() {
    let folder = Folder::new("errors");
    folder.write("housefix.toml", "pending = '''\n-  Fix the door\n'''\n");
    let missing = folder.run(&["schedule", "bils"]);
    assert!(
        stderr(&missing).starts_with("error: no topic bils: no bils.toml in "),
        "{}",
        stderr(&missing)
    );
    assert_eq!(missing.status.code(), Some(2));
    let no_reminder = folder.run(&["mark", "housefix", "Paint"]);
    assert_eq!(stderr(&no_reminder), "error: no reminder \"Paint\" in housefix\n");
    let early = terminalert(&["--folder", folder.dir.to_str().unwrap(), "show"], None, None);
    assert!(stderr(&early).contains("options go after the command"), "{}", stderr(&early));
    assert_eq!(early.status.code(), Some(2));
}

/// With no folder handed over, `TERMINALERT_CONFIG_DIR` is the folder.
#[test]
fn the_folder_comes_from_the_environment() {
    let folder = Folder::new("environment");
    folder.write("appointments.toml", &format!("pending = '''\n{}  Dentist\n'''\n", full_date(0)));
    let shown = terminalert(&["show"], None, Some(&folder.dir));
    assert_eq!(stdout(&shown), "Due today: appointments:Dentist\n");
}
