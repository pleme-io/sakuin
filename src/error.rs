use std::fmt;

#[derive(Debug)]
pub enum TankyuError {
    Tantivy(tantivy::TantivyError),
    QueryParser(tantivy::query::QueryParserError),
    Io(std::io::Error),
    WriterBusy,
}

impl fmt::Display for TankyuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tantivy(e) => write!(f, "tantivy: {e}"),
            Self::QueryParser(e) => write!(f, "query parser: {e}"),
            Self::Io(e) => write!(f, "io: {e}"),
            Self::WriterBusy => write!(f, "index writer lock poisoned"),
        }
    }
}

impl std::error::Error for TankyuError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Tantivy(e) => Some(e),
            Self::QueryParser(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::WriterBusy => None,
        }
    }
}

impl From<tantivy::TantivyError> for TankyuError {
    fn from(e: tantivy::TantivyError) -> Self {
        Self::Tantivy(e)
    }
}

impl From<tantivy::query::QueryParserError> for TankyuError {
    fn from(e: tantivy::query::QueryParserError) -> Self {
        Self::QueryParser(e)
    }
}

impl From<std::io::Error> for TankyuError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
