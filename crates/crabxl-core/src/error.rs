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
    /// No visible sheet is available for workbook serialization.
    NoVisibleSheet,
    /// Managed operation allocations exceeded the selected memory allowance.
    MemoryBudgetExceeded,
    /// The requested worksheet does not exist.
    SheetNotFound,
    /// An operation conflicts with a resource or sequential-mode state.
    InvalidState,
}

/// A spreadsheet error retaining the affected part, cell, and underlying cause.
/// Details are allocated only on failure, keeping successful Result values small.
pub struct Error {
    details: Box<ErrorDetails>,
}

struct ErrorDetails {
    kind: ErrorKind,
    message: String,
    part: Option<String>,
    cell: Option<crate::CellAddress>,
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl Error {
    /// Construct an error without an underlying cause.
    #[cold]
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            details: Box::new(ErrorDetails {
                kind,
                message: message.into(),
                part: None,
                cell: None,
                source: None,
            }),
        }
    }

    /// Preserve a concrete cause without exposing format-specific types in core.
    #[cold]
    pub fn caused_by(
        kind: ErrorKind,
        message: impl Into<String>,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        let mut error = Self::new(kind, message);
        error.details.source = Some(Box::new(source));
        error
    }

    /// Attach the affected archive part.
    pub fn with_part(mut self, part: impl Into<String>) -> Self {
        self.details.part = Some(part.into());
        self
    }

    /// Attach the affected cell coordinate.
    pub fn with_cell(mut self, cell: crate::CellAddress) -> Self {
        self.details.cell = Some(cell);
        self
    }

    /// Return the matchable error category.
    pub fn kind(&self) -> ErrorKind {
        self.details.kind
    }
    /// Return the affected archive part, when known.
    pub fn part(&self) -> Option<&str> {
        self.details.part.as_deref()
    }
    /// Return the affected cell, when known.
    pub fn cell(&self) -> Option<crate::CellAddress> {
        self.details.cell
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.details.message)?;
        if let Some(part) = &self.details.part {
            write!(f, " [part: {part}]")?;
        }
        if let Some(cell) = self.details.cell {
            write!(f, " [cell: {cell}]")?;
        }
        Ok(())
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.details
            .source
            .as_deref()
            .map(|e| e as &(dyn StdError + 'static))
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("kind", &self.details.kind)
            .field("message", &self.details.message)
            .field("part", &self.details.part)
            .field("cell", &self.details.cell)
            .field("source", &self.details.source)
            .finish()
    }
}
