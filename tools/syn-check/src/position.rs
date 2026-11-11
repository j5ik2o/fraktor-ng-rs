use proc_macro2::LineColumn;

/// A location in a source file. Both fields are 1-based, as rustc prints them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    /// The first character of a file.
    pub const START: Position = Position { line: 1, column: 1 };

    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

impl From<LineColumn> for Position {
    fn from(value: LineColumn) -> Self {
        Self {
            line: value.line,
            column: value.column + 1,
        }
    }
}
