//! Character columns of byte offsets in one line, for a language reading
//! the tokens of a line in order (hover, the caret's key).

/// Columns of byte offsets asked in rising order: each counted on from the
/// one asked before, so a line of many tokens is read once, not once per
/// token (P5: a long line would cost its square).
pub(super) struct Columns<'a> {
    line: &'a str,
    byte: usize,
    column: usize,
}

impl<'a> Columns<'a> {
    pub(super) fn new(line: &'a str) -> Self {
        Self {
            line,
            byte: 0,
            column: 0,
        }
    }

    /// The column of `byte` (a character boundary of the line); an offset
    /// before the last one asked counts again from the line's start.
    pub(super) fn at(&mut self, byte: usize) -> usize {
        if byte < self.byte {
            crate::editor::work_count::note_many(
                crate::editor::work_count::Work::ColumnRecount,
                byte,
            );
            self.byte = 0;
            self.column = 0;
        }
        self.column += self.line[self.byte..byte].chars().count();
        self.byte = byte;
        self.column
    }
}

#[cfg(test)]
mod tests {
    use super::Columns;

    #[test]
    fn columns_count_characters_in_any_order() {
        let line = "aé😀b";
        let mut columns = Columns::new(line);
        assert_eq!(columns.at(1), 1);
        assert_eq!(columns.at(3), 2);
        assert_eq!(columns.at(line.len()), 4);
        assert_eq!(columns.at(1), 1);
        assert_eq!(columns.at(0), 0);
    }
}
