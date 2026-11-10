//! A table line, split into its columns by `table_formatter`'s rule: two or more spaces, or a
//! tab, between columns — so a single space stays inside one (`2026-11-20 10:30`, `Pay invoice
//! #1234`). A column starting with `#` is the note, and runs to the end of the line.

/// A line's columns, and its note.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Row<'a> {
    /// The columns before the note.
    pub columns: Vec<&'a str>,
    /// The note: what follows a column's opening `#`, to the end of the line. `None` for a line
    /// without one, or with nothing after its `#`.
    pub note: Option<&'a str>,
}

/// The columns of `line` — `None` for a line that holds no reminder: a blank one, or a comment (a
/// `#` first).
pub(crate) fn split(line: &str) -> Option<Row<'_>> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut row = Row { columns: Vec::new(), note: None };
    let mut rest = line;
    loop {
        if let Some(note) = rest.strip_prefix('#') {
            row.note = Some(note.trim()).filter(|note| !note.is_empty());
            return Some(row);
        }
        let (column, after) = match separator(rest) {
            Some(gap) => (&rest[..gap.start], rest[gap.end..].trim_start()),
            None => (rest, ""),
        };
        row.columns.push(column);
        if after.is_empty() {
            return Some(row);
        }
        rest = after;
    }
}

/// Where the first separator in `text` is: a run of whitespace that is two spaces or more, or
/// holds a tab.
fn separator(text: &str) -> Option<std::ops::Range<usize>> {
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if c != ' ' && c != '\t' {
            continue;
        }
        let mut end = start + c.len_utf8();
        let mut tabbed = c == '\t';
        while let Some(&(at, next)) = chars.peek().filter(|(_, next)| *next == ' ' || *next == '\t')
        {
            tabbed |= next == '\t';
            end = at + next.len_utf8();
            chars.next();
        }
        if tabbed || end - start >= 2 {
            return Some(start..end);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn columns(line: &str) -> (Vec<&str>, Option<&str>) {
        let row = split(line).expect("a reminder's line");
        (row.columns, row.note)
    }

    /// Two spaces or more, or a tab, part columns; one space stays inside a column.
    #[test]
    fn columns_part_at_two_spaces_or_a_tab() {
        assert_eq!(
            columns("2026-11-20 10:30  Dentist"),
            (vec!["2026-11-20 10:30", "Dentist"], None)
        );
        assert_eq!(columns("* * 9 2-12/2 *    Water"), (vec!["* * 9 2-12/2 *", "Water"], None));
        assert_eq!(columns("mon\tTake out the trash"), (vec!["mon", "Take out the trash"], None));
        assert_eq!(columns("mon \tTrash"), (vec!["mon", "Trash"], None), "a tab among spaces");
        assert_eq!(columns("04-12 Dana"), (vec!["04-12 Dana"], None), "one space: one column");
    }

    /// A column opening with `#` is the note — the rest of the line, its own spacing kept; a
    /// `#` inside a column is just text.
    #[test]
    fn a_column_opening_with_a_hash_is_the_note() {
        assert_eq!(
            columns("last  Electricity    # 5% off  if paid by the 25th"),
            (vec!["last", "Electricity"], Some("5% off  if paid by the 25th"))
        );
        assert_eq!(columns("-  Pay invoice #1234"), (vec!["-", "Pay invoice #1234"], None));
        assert_eq!(columns("-  Fix it  #"), (vec!["-", "Fix it"], None), "an empty note is none");
        assert_eq!(columns("-  Fix it  #tight"), (vec!["-", "Fix it"], Some("tight")));
    }

    /// Blank lines and comments hold no reminder; indentation and trailing space don't count.
    #[test]
    fn blank_lines_and_comments_hold_no_reminder() {
        for nothing in ["", "   ", "# Timing  Reminder", "   # indented comment"] {
            assert_eq!(split(nothing), None, "{nothing:?}");
        }
        assert_eq!(columns("   mon   Trash   "), (vec!["mon", "Trash"], None));
    }

    /// More than two columns stay apart, for the reader to report.
    #[test]
    fn every_column_is_kept() {
        assert_eq!(columns("1  Rent  monthly"), (vec!["1", "Rent", "monthly"], None));
    }
}
