use std::{error::Error as StdError, fmt};

/// A result with a context-rich spreadsheet error.
pub type Result<T> = std::result::Result<T, Error>;

/// Stable categories suitable for matching in Rust or language adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Input/output failed.
    Io,
    /// The ZIP container could not be read.
    Archive,
    /// XML syntax or namespace resolution failed.
    Xml,
    /// Spreadsheet content violates its format or coordinate rules.
    InvalidData,
    /// A feature is not implemented by the selected mode yet.
    Unsupported,
    /// A configured resource budget was exceeded.
    LimitExceeded,
    /// Retained sheet data exceeded the operation's memory budget.
    MemoryBudgetExceeded,
    /// The requested worksheet does not exist.
    SheetNotFound,
}

/// A spreadsheet error retaining the affected part, cell, and underlying cause.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    part: Option<String>,
    cell: Option<crate::CellAddress>,
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl Error {
    /// Construct an error without an underlying cause.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            part: None,
            cell: None,
            source: None,
        }
    }

    /// Preserve a concrete cause without exposing format-specific types in core.
    pub fn caused_by(
        kind: ErrorKind,
        message: impl Into<String>,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        Self {
            source: Some(Box::new(source)),
            ..Self::new(kind, message)
        }
    }

    /// Attach the affected archive part.
    pub fn with_part(mut self, part: impl Into<String>) -> Self {
        self.part = Some(part.into());
        self
    }

    /// Attach the affected cell coordinate.
    pub fn with_cell(mut self, cell: crate::CellAddress) -> Self {
        self.cell = Some(cell);
        self
    }

    /// Return the matchable error category.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
    /// Return the affected archive part, when known.
    pub fn part(&self) -> Option<&str> {
        self.part.as_deref()
    }
    /// Return the affected cell, when known.
    pub fn cell(&self) -> Option<crate::CellAddress> {
        self.cell
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(part) = &self.part {
            write!(f, " [part: {part}]")?;
        }
        if let Some(cell) = self.cell {
            write!(f, " [cell: {cell}]")?;
        }
        Ok(())
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_deref()
            .map(|e| e as &(dyn StdError + 'static))
    }
}
