//! Where the topics live: terminalert's own folder, or whichever folder it is handed — as bashrs
//! hands it its own — the topics in it, and the example files a new one starts with.

use std::io;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::topic::{Problem, Topic};
use crate::words;

/// The folder when none is handed over: `$TERMINALERT_CONFIG_DIR`, else
/// `$XDG_CONFIG_HOME/terminalert`, else `~/.config/terminalert` — found as sequencer finds its
/// configuration folder.
pub fn default_folder() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("TERMINALERT_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Ok(PathBuf::from(xdg).join("terminalert"));
    }
    let home = std::env::var_os("HOME").ok_or(Error::NoFolder)?;
    Ok(PathBuf::from(home).join(".config").join("terminalert"))
}

/// The topics' files in `folder`, in name order: its own `*.toml` files — a link to one counting
/// as one — and nothing hidden, nothing in a subfolder (`examples/` among them), nothing else (a
/// `.done` file, an editor's `bills.toml~`). A folder that isn't there has none.
pub fn topic_files(folder: &Path) -> Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(Error::Read { path: folder.to_owned(), source }),
    };
    let is_topic = |path: &PathBuf| {
        let hidden = path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.'));
        let suffix =
            path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case(words::TOPIC_SUFFIX));
        !hidden && suffix && path.is_file()
    };
    let mut files: Vec<PathBuf> =
        entries.flatten().map(|entry| entry.path()).filter(is_topic).collect();
    files.sort();
    Ok(files)
}

/// Every topic in `folder`, read, in name order. A file that can't be read is a topic still —
/// one with no reminders and that one problem — so it is reported with the rest.
pub fn topics(folder: &Path) -> Result<Vec<Topic>> {
    let read = |path: PathBuf| {
        Topic::read(&path).unwrap_or_else(|err| {
            let mut topic = Topic::parse(&path, String::new());
            topic.problems.push(Problem { line: None, message: err.to_string() });
            topic
        })
    };
    Ok(topic_files(folder)?.into_iter().map(read).collect())
}

/// The topic named `name` in `folder`.
pub fn topic(folder: &Path, name: &str) -> Result<Topic> {
    let path = folder.join(format!("{name}.{}", words::TOPIC_SUFFIX));
    if !path.is_file() {
        return Err(Error::NoTopic { name: name.to_owned(), folder: folder.to_owned() });
    }
    Topic::read(&path)
}

/// The example files, as this repository's `examples/` holds them.
pub const EXAMPLES: &[(&str, &str)] = &[
    ("appointments.toml", include_str!("../examples/appointments.toml")),
    ("bills.toml", include_str!("../examples/bills.toml")),
    ("birthday.toml", include_str!("../examples/birthday.toml")),
    ("deadlines.toml", include_str!("../examples/deadlines.toml")),
    ("housefix.toml", include_str!("../examples/housefix.toml")),
    ("routine.toml", include_str!("../examples/routine.toml")),
];

/// Give `folder` the example files, in its `examples/` — unless it has that already: once there,
/// they're the user's to keep, change or delete. Returns whether they were written.
pub fn write_examples(folder: &Path) -> io::Result<bool> {
    let examples = folder.join(words::EXAMPLES_DIR);
    if examples.symlink_metadata().is_ok() {
        return Ok(false);
    }
    std::fs::create_dir_all(&examples)?;
    for (name, text) in EXAMPLES {
        std::fs::write(examples.join(name), text)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("terminalert_folder_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// [`EXAMPLES`] is the repository's `examples/`, file for file: a new example is read, or
    /// this fails.
    #[test]
    fn the_examples_are_the_examples_folder() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let embedded: Vec<&str> = EXAMPLES.iter().map(|(name, _)| *name).collect();
        assert_eq!(on_disk, embedded);
        for (name, text) in EXAMPLES {
            assert_eq!(std::fs::read_to_string(dir.join(name)).unwrap(), *text, "{name}");
        }
    }

    /// Every example reads with no problem: they double as the reader's tests.
    #[test]
    fn every_example_reads_cleanly() {
        for (name, text) in EXAMPLES {
            let topic = Topic::parse(Path::new(name), (*text).to_owned());
            assert!(topic.problems.is_empty(), "{name}: {:?}", topic.problems);
            assert!(!topic.reminders.is_empty(), "{name}");
        }
    }

    /// The folder's own `.toml` files are its topics, in name order — nothing hidden, nothing
    /// in a subfolder, no `.done` file, no leftover — and a missing folder has none.
    #[test]
    fn only_the_folder_s_own_toml_files_are_topics() {
        let dir = scratch("topics");
        for name in [
            "bills.toml",
            "Birthday.TOML",
            "bills.done",
            "bills.toml~",
            ".hidden.toml",
            "notes.txt",
        ] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        std::fs::create_dir_all(dir.join("examples")).unwrap();
        std::fs::write(dir.join("examples/routine.toml"), "").unwrap();
        std::fs::create_dir_all(dir.join("folder.toml")).unwrap();
        std::os::unix::fs::symlink(dir.join("bills.toml"), dir.join("linked.toml")).unwrap();
        let names: Vec<String> = topic_files(&dir)
            .unwrap()
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["Birthday.TOML", "bills.toml", "linked.toml"]);
        assert!(topic_files(&dir.join("never-made")).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The examples are written once, into `examples/`; after that they're the user's.
    #[test]
    fn the_examples_are_written_once() {
        let dir = scratch("examples");
        assert!(write_examples(&dir).unwrap());
        assert_eq!(std::fs::read_dir(dir.join("examples")).unwrap().count(), EXAMPLES.len());
        std::fs::remove_file(dir.join("examples/bills.toml")).unwrap();
        assert!(!write_examples(&dir).unwrap(), "not again");
        assert!(!dir.join("examples/bills.toml").exists(), "the user's deletion stands");
        assert!(topic_files(&dir).unwrap().is_empty(), "examples aren't topics");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A topic's file that can't be read is reported as a problem of that topic.
    #[test]
    fn an_unreadable_file_is_a_problem_not_a_stop() {
        let dir = scratch("unreadable");
        std::fs::write(dir.join("ok.toml"), "pending = '''\n-  Fix it\n'''\n").unwrap();
        std::os::unix::fs::symlink(dir.join("gone"), dir.join("dangling.toml")).unwrap();
        std::fs::write(dir.join("binary.toml"), [0xff, 0xfe, 0x00]).unwrap();
        let topics = topics(&dir).unwrap();
        let names: Vec<&str> = topics.iter().map(|topic| topic.name.as_str()).collect();
        assert_eq!(names, ["binary", "ok"], "a dangling link isn't a file");
        assert_eq!(topics[0].problems.len(), 1);
        assert!(topics[0].problems[0].message.contains("can't read"), "{:?}", topics[0].problems);
        assert_eq!(topics[1].reminders.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
