# terminalert — requirements and ideas

The plan for terminalert, kept while it is designed and built: what is decided, what is still
open, and the ideas behind both — the small details included, as they come up. Decisions are dated
so a later reader knows how settled each is. The interface comes first — it is a convenience app,
so how it reads and feels decides the rest — and the code after it (developer's call, 2026-11-06).

## What it is

A reminder system for the terminal — half calendar, half to-do list. The user keeps reminders in
files, one topic per file (birthdays, bills, appointments, house fixes…), and terminalert says what
is due: what is coming up, what is due now, and what is still waiting to be done. What the user has
dealt with is pushed aside, not lost.

## The words (decided 2026-11-10)

A file is a **topic** (`bills.toml` is the topic "bills"); its table's rows are **reminders**,
each a **Timing** and a **Reminder** column: when it is due, and what to remind of — an event (a
birthday, an appointment) as well as a thing to do (a bill, a chore). A timing's day or moment is
its **date**. In the code, a reminder with something to say right now is an **alert**. The words
before: subject, Task, Deadline — the `.done` files still read entries written under them.

## Requirements (decided 2026-11-06)

1. **Easy to read and to add to.** As few columns as possible: a reminder is a table row — its
   Timing, and the Reminder — like a crontab line. Not a calendar format: the files read as lists.
   Anything more about a reminder (a second deadline, a word to oneself) goes, preferably, in a
   note at the end of its row — `Dentist  # bring the X-rays` — or else in the reminder's own words.
2. **One file per topic, settings per file.** A file's reminders are alike, so what would otherwise
   be columns is said once, at the top of the file, and a reminder that needs other settings goes
   in another file — no per-reminder exceptions:
   - `recurring` — a plain boolean, no frequency: whether the file's reminders come back after
     they're handled (bills, birthdays), or are a to-do list (appointments, house fixes).
   - `notice` — how long ahead of a reminder's date to start reminding of it.
   - `sticky` — whether a reminder keeps reminding after its date until it is dealt with (a bill, a
     deadline), or lapses on its own once its date has passed (a birthday, after the day).
   - `urgency` — a number in the file, a word when printed: 1 whenever, 2 can-wait, 3 soon,
     4 pressing, 5 ASAP. It orders the reminders (requirement 12).

   Each is optional: its default is the project's own, in `template.toml` (requirement 15) —
   `recurring = false`, `notice = "1 day"`, `sticky = false`, `urgency = 3`.
3. **The timing gives the reminder's dates; `notice` is measured back from them.** A bill due on
   the 20th with a notice of five days reminds from the 15th.
4. **Chores — reminders without a timing — have `-` in its place**, and belong to to-do files:
   pending until done.
5. **Timings**: a date — `2026-11-20` once, `04-12` every year — and for anything else cron, an
   existing system rather than one of our own: `* * 9 2-12/2 *` is the 9th of every second month,
   from February. Cron as crontab writes it — five fields, minute first — with `* *` for the time
   meaning all day (`* * 1 * *`), and a time otherwise (`30 9 1 * *`). A few short forms stay:
   `daily`, `mon` (or `mon,thu`), and `last`, a month's last day; after a date or a short form, a
   time may follow (`2026-11-20 10:30`, `mon 18:00`). Recurring files take repeating timings — a
   full date there repeats every year — and to-do files take full dates, or `-`. `template.toml`
   specifies cron's usages in detail, and the examples make the order unmistakable (month before
   day), as sequencer's `example_profile.toml` teaches its format.
6. **TOML holds the table**: the pending reminders under a key of their own, `pending`, between
   `'''` marks — taken as typed, where between `"""` a backslash starts an escape and `C:\Users`
   fails to load. Columns are split by two or more spaces, or a tab — `table_formatter`'s rule — so
   a single space stays inside a column (`2026-11-20 10:30`, `Pay invoice #1234`); a column starting
   with `#` is the reminder's note.
7. **Done is pushed aside, never deleted**: a finished reminder moves into `<topic>.done` —
   `bills.done` beside `bills.toml` — terminalert's alone, never the user's to write, stamped with
   when it was marked (done, or skipped); and it can be moved back (undo). Only `.toml` files are
   topics, so a `.done` file is never read as one.
8. **A recurring reminder can be handled for one round**: the next date reminds again. The round is
   recorded in the `.done` file while the reminder stays pending — one entry per reminder, found by
   what it says and overwritten each round; so two reminders alike in one file are an error. A
   reminder with no entry there is pending from its latest date, however long ago: a sticky
   `03-01  Car tax` written in November shows as overdue until it is marked once — and undoing a
   round, which leaves no entry, makes that round pending again.
9. **Where the files live**: terminalert has a folder of its own, found as sequencer finds its
   config folder — `$TERMINALERT_CONFIG_DIR`, else `$XDG_CONFIG_HOME/terminalert`, else
   `~/.config/terminalert` — and bashrs hands it `~/.bashrs/user-custom/terminalert/` instead, as a
   bare `autokey_apply` hands sequencer `~/.bashrs/user-custom/sequencer` (bashrs moves none of
   sequencer's folders: it passes its own). The folder holds the user's files and, beside each, its
   `.done` file; the defaults stay in the project (requirement 15). A first install creates the
   folder, with the example files in `examples/` (not read as topics, so their made-up reminders
   never show).
10. **When reminders show**: the sourcefile ends with one simple call to terminalert, the same every
    time — a command that checks the user's files and prints what is due — made where bashrs's red
    warning for outdated user files is: at the very end, in a terminal's first shell. The alerts
    print before that warning, which stays the last thing before the prompt (bashrs's
    `conf/greeting.rs`). Nothing of the reminders is kept in bashrs. The same command runs on
    demand, and keyboard shortcuts are to come: to view, edit, remove, and to expand the counts
    (requirement 11).
11. **How alerts read**:
    - **Sticky reminders are counted**, per file — chores always, and a sticky reminder with a
      timing from its notice on — and a count says how many of them are overdue, in a colour of
      its own: "You have 2 bills (1 overdue), 5 housefix and 3 organizing reminders pending" (from
      `bills.toml`, `housefix.toml`, `organizing.toml`). A keyboard shortcut lists them in full.
    - **The rest are listed**: one as a sentence — "Upcoming in 1 day: birthday:Dana" (from
      `birthday.toml`); several as a table, with headers.
    - **Notes show** with a listed reminder — not in a count.
    - **The table's columns**: When, Topic, Reminder, Note — the topic before the reminder, as the
      one-line alert writes them, `birthday:Dana` (developer's call, 2026-11-09). "When" there is
      how soon ("in 1 day", "overdue since…"), not the Timing column's rule.
    - **What is due now and what is coming up** in different colours.
12. **Order**: by urgency — the counted files, and the listed reminders (ascending or descending:
    whichever looks better, later); within a file, the reminders in the order they're written.
13. **No snooze.** "Remind me later" is not in the specification.
14. **Dates through `jiff`** (two packages; time zones and daylight-saving changes handled). Its
    friendly durations are `notice`'s values: `"1 day"`, `"2 weeks"`, `"12 hours"`. **Cron
    through `croner`** 4.0.3, on jiff — its `jiff` feature, without its default chrono (decided
    2026-11-06): crontab's five fields, names, and cron's either-day rule. MIT; released
    2026-11-04; 12.3M downloads, 5.5M in 90 days; 12 contributors; no RustSec advisories. It adds
    11 packages to bashrs's tree: croner, and the build-time macros under it (`derive_builder`,
    `darling`, `strum`). Passed over: `cron`, the most downloaded, wants a seconds field and
    chrono; `saffron` has had no release since 2021; writing our own.
15. **`template.toml` and `examples/`**, as sequencer's `example_profile.toml` and
    `practical_profile_examples/`. `template.toml`, at the repository root, is the format
    reference — every field once, at its default, explained in comments, concisely — and holds the
    project's defaults; it is never copied among the user's files. `examples/` holds practical
    files, each commented to teach writing such a file, not how terminalert works. Both are to
    double as the parser's tests, as sequencer's reference does.
16. **A topic's schedule, in words**: a command prints a topic's whole schedule, each timing
    spelled out — "the 9th of February, April, June…" — so the user can make sure it says what
    they meant. An idea with it: it is also where a timing that can't be read gets reported, as
    sequencer's `profile-check` reports a profile's problems.

## The cost at shell start

**The call at shell start is a process**, and bashrs budgets shell start-up by measurement: it
prints its greeting with the shell's builtins because its binary cost ~0.9 ms at each terminal's
start (`conf/greeting.rs`), and has kept other start-up additions near 0.3 ms. Measured
(2026-11-06; release builds, 1000 runs a round, three rounds alike):

| What runs | Cost |
|---|---|
| a program that does nothing (`/bin/true`) | ~0.25 ms |
| an empty Rust program | ~0.3 ms |
| bashrs's binary doing nothing (`bashrs --version`) | ~0.9 ms |
| `terminalert show`, its own binary, over the six example files | ~0.9 ms |
| `bashrs remind`, the call the sourcefile makes, over the same files | ~1.4 ms |
| `rayon`'s first use, on a 32-thread CPU (it starts a thread per core) | ~0.55 ms more |

- **The call costs ~1.4 ms a terminal** (its first shell only; a shell started from it pays
  nothing): bashrs's own start, and ~0.5 ms of terminalert's reading and working out. Above both
  the ~0.3 ms bashrs has accepted and the ~0.9 ms it rejected for the greeting — the developer's
  to weigh (2026-11-06). Ways down, none taken yet: terminalert's own binary at shell start
  (~0.5 ms less, but no Rust companion is installed on its own today); and terminalert's own work,
  once profiled — a timing's "never comes" check, for one, only `schedule` needs.
- **The alerts' table** isn't printed through `table_formatter`: it always works through rayon,
  which would cost the more again.

## Kinds of reminders

| | Timing | After its date | Shows as |
|---|---|---|---|
| A sticky recurring reminder (rent) | repeating | reminds until marked | counted, from `notice` before |
| A recurring reminder (a birthday) | repeating | lapses | listed, from `notice` before |
| A sticky dated to-do (a deadline) | once | reminds until marked | counted, from `notice` before |
| A dated to-do (the dentist) | once | lapses | listed, from `notice` before |
| A chore (fix the door) | `-` | — | counted, always |

## The interface (proposed, 2026-11-06)

### Files

`template.toml` and `examples/` (requirement 15) — the files' format lives there alone, so no
second copy here can drift from it.

### Alerts

At the end of the sourcefile — the counted reminders, and one listed reminder:

```
Upcoming in 1 day: birthday:Dana
You have 2 bills (1 overdue), 5 housefix and 3 organizing reminders pending.
```

Several listed reminders, as a table with headers:

```
When        Topic          Reminder   Note
today       appointments   Dentist    bring the X-rays
in 1 day    birthday       Dana
in 5 days   birthday       Grandpa

You have 2 bills (1 overdue), 5 housefix and 3 organizing reminders pending.
```

### Marking reminders (command names: later)

- **Mark**: tick reminders in a form (`terminal_choice`'s, as bashrs's `murder` offers processes),
  then *done* or *skipped*. In a recurring file that handles this round; in a to-do file the
  reminder moves to its `.done` file. A recurring reminder can also be taken out *for good*: it
  moves there too.
- **Undo**: pick what was marked — a handled round, or a finished reminder — to put it back.

## Open questions

1. **Command names** — the working ones, until the system has been used: `terminalert` (alone,
   `show`), `list`, `mark`, `undo`, `schedule`; in bashrs, `remind` and `remind_list`,
   `remind_mark`, `remind_undo`, `remind_schedule`.
2. **The alerts' look** — where urgency's word shows (a column, a heading per level…), which way
   the order runs, the colours, the keys. The count's sentence says "reminders pending" since the
   rename (it said "tasks pending" before).
3. **A fresh sticky file starts overdue**: written today, a sticky recurring reminder is pending
   from its latest date (requirement 8) — so the bills example, copied in as it is, reports every
   bill overdue until each is marked once. As decided; worth seeing in use.
4. **The cost at shell start**: ~1.4 ms a terminal, measured — keep it, or bring it down (see *The
   cost at shell start*).

## Built in (2026-11-06)

Proposed with the plan and not objected to, these are in the code:

- **An all-day timing's date is its whole day**: `notice` counts back from the day's start, and
  the reminder lapses — or, sticky, turns overdue — when the day ends. A timing with a time is its
  moment.
- **Time is the system's local time**, through jiff.
- **Files of equal urgency** go by their topic's name.
- **Only the folder's own `*.toml` files are topics** — not its subfolders (`examples/` among
  them), its `.done` files, or an editor's leftovers (`bills.toml~`).
- **A misspelled setting is an error** (`stiky = true`), as sequencer and devmounts refuse unknown
  keys.
- **A broken file costs one short line** at shell start; the schedule command gives the details.
- **Marking a recurring reminder handles it through its latest date** — its one `.done` entry is
  overwritten — so missed rounds don't pile up.

## Plan of the code (2026-11-06)

1. **Done — the crate as devmounts lays it out**: a library, its clap front end behind a `cli`
   feature (on by default; terminal_choice rides with it), and a binary. Beside jiff and croner:
   serde, `toml`, thiserror. `toml_edit` wasn't needed: an edit is a line taken out of the table,
   or put back at its end, and between `'''` marks the table's text in the file IS its value — so
   the rest of the file is kept byte for byte without it.
2. **Done — reading the files**: settings, the table, timings and notes, every problem on its line,
   and the rest still read; `template.toml` and `examples/` are the reader's tests, and
   `template.toml` is where the defaults are read from.
3. **Done — what is due**: dates in time, `notice`, sticky and overdue, the `.done` records — the
   present moment passed in, so tests pin it.
4. **Done — the commands** (working names): the alerts, plain for now; the counted reminders in
   full; marking and undo, picked in a `terminal_choice` form or named (`mark bills Rent`); the
   schedule in words.
5. **Done — bashrs, as a local path dependency** (the developer's call: sooner, to test with): the
   reminder store as a `Place` under `user-custom`, the install creating it with the examples in
   its `examples/` once, the `remind` call in a terminal's first shell between the greeting and
   the warning, and the `remind*` commands handing terminalert the store as its folder.
6. **Done — the words** (2026-11-10): topic, Reminder, Timing, through the code, its messages and
   names, the template and the examples.
7. **Next, with the developer**: the alerts' look and the keys.

### Where to change what

Every name is a working name, each in one place:

- the format's words — the short forms, urgency's words, the files' suffixes, the columns' names:
  `src/words.rs`;
- a setting's key: its field in `src/settings.rs`;
- the output's wording and look: `src/render.rs`;
- a command's name: its variant in `src/cli.rs` (and its `#[name]` in bashrs's `comfy_repos.rs`).

### Found on the way (2026-11-06)

- **croner's own wording can't be the schedule's**: its `describe()` says "in every 2 months" for
  `*/2` and `2-12/2` alike. The schedule names the months instead ("the 9th of February, April,
  June…"), from croner's own reading of the pattern.
- **croner reads more than crontab**: `L`, `W`, `#`, `?`, a seconds field, the `@daily` words.
  terminalert refuses them, as cron as crontab writes it; `last` is the short form for `L`.
- **A table between `"""` marks is read, but not edited**: a backslash there is an escape, so its
  text in the file isn't its value. Marking a to-do in such a file says to use `'''`.
- **A file changed since it was read is never written over**: the edit is refused, and the record
  rolled back.
- **Undo puts a reminder back at the end of its table**: where it stood isn't kept.
- **The `.done` file** is `[[entry]]` tables — `reminder`, `marked`, `as`, `due` (the round's
  date), `row` (a reminder taken out, for undo) — terminalert's alone. Entries written before the
  rename, with `task` and `deadline`, still read.

## Rejected (2026-11-06)

- **A plain table file with a `[done]` section** — TOML holds the table instead.
- **Cron's fields for every timing** — no year for a one-off date, and a time of day even for a
  reminder due all day. Dates stay dates; cron writes the rest.
- **Our own repeat syntax** (`*-*/2-10`: year-month-day with cron's `*`, `/` and `,` inside) —
  confusing next to cron, which says the same.
- **`notice` and `sticky` as columns** — they're per file, for fewer columns.
- **A frequency setting per file** — `recurring` is a boolean; when a reminder comes back is its
  own timing.
- **Per-reminder exceptions** — a reminder of other settings goes in another file.
- **A precomputed summary printed by the shell's builtins** at the end of the sourcefile — the
  sourcefile makes one simple call to terminalert instead, and keeps nothing of the reminders.
- **A settings file among the user's files** — the defaults are the project's, in `template.toml`.
- **A done table in the topic's file**, and a record of handled rounds in
  `~/.bashrs/user-appdata/` — finished reminders and handled rounds go to `<topic>.done`.
- **An empty timing for a chore** — `-` marks it.
- **Short forms cron already says**: a bare day (`12`, cron's `* * 12 * *`), `02-last`, `2/2`.
- **Cron's `@daily`-style words** — `@daily` is `0 0 * * *` (crontab(5)), one moment at midnight,
  not the whole day: a reminder there would pass as its day begins. `daily` says the whole day.
- **"subject", "Task", "Deadline"** (2026-11-10) — a birthday isn't a task, nor its date a
  deadline. Weighed with them: list (Apple Reminders' and Microsoft To Do's word, but it clashes
  with the `list` command), category; item, entry, what, matter, memo; When (the output's).
- **Snooze.**

## Later

- **The years since a date**: in a recurring file whose timings are all a day of the year
  (`04-12`, `1990-04-12`, or cron naming one day a year), a full date's alert adds how many years
  have passed: "(36 years)" — only for a date already past; a date still to come gets no count
  (developer's call, 2026-11-06). Open with it: a birthday reminded a week ahead counts the years
  at its date (the age Dana turns), or at the moment it's printed?
- **Every other week** — cron can't say it; a short form for it, if wanted.
- **Importing `.ics` exports** from calendar apps.
- **Editing reminders through forms** — bashrs's backlog: "Calendaric notifications and notes
  could go into comfygurations".
