use core::fmt;
use core::num::{ParseIntError, IntErrorKind};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive] // So that we can add more error types later without breaking compatibility
pub enum CustomError {
    MathOverflow,
    ParseIntError(IntErrorKind),
    FormatError,
    BadInput,

    Unimplemented,
    Other
}

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Use Debug to print the variant name generically
        write!(f, "{:?}", self)
    }
}

impl core::error::Error for CustomError {}

impl From<ParseIntError> for CustomError {
    fn from(err: ParseIntError) -> Self {
        /* For SOME FUCKING REASON, the idiotic IntErrorKind enum doesn't implement Copy, because of course it doesn't. That's too much to ask for.
        Even though it only contains unit variants, so it has no actual data in it.
        So we have to clone it. Unbe-fucking-lievable. */
        CustomError::ParseIntError(err.kind().clone())
    }
}

impl From<fmt::Error> for CustomError {
    fn from(_: fmt::Error) -> Self {
        CustomError::FormatError
    }
}

impl From<()> for CustomError {
    fn from(_: ()) -> Self {
        CustomError::Other
    }
}