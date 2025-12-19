use core::fmt;
use core::num::{ParseIntError, IntErrorKind};
use display_interface::DisplayError;
use heapless::CapacityError;
use rp2040_hal::uart::ReadErrorType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format, Default)]
#[non_exhaustive] // So that we can add more error types later without breaking compatibility
pub enum CustomError {
    MathOverflow,
    ParseIntError(IntErrorKindClone),
    FormatError,
    BadInput,

    DisplayError(DisplayErrorClone),
    CapacityError,

    UartReadError(ReadErrorTypeClone),

    Unimplemented,
    Impossible,
    Cancelled,
    #[default] Other // We have to mark a default variant by this attribute for the Default derive
}

// Because IntErrorKind doesn't implement defmt::Format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum IntErrorKindClone {
    Empty,
    InvalidDigit,
    PosOverflow,
    NegOverflow,
    Zero,
}

// Because DisplayError doesn't implement PartialEq nor Eq, or at least until my PR gets merged. (It should implement defmt::Format though.)
// Said PR: https://github.com/therealprof/display-interface/pull/55
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum DisplayErrorClone {
    InvalidFormatError,
    BusWriteError,
    DCError,
    CSError,
    DataFormatNotImplemented,
    RSError,
    OutOfBoundsError,
}

// Because ReadErrorType doesn't implement Clone, Copy, PartialEq nor Eq. (It should implement defmt::Format though.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum ReadErrorTypeClone {
    Overrun,
    Break,
    Parity,
    Framing
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
        let kind = *err.kind();
        
        CustomError::ParseIntError(match kind {
            IntErrorKind::Empty => IntErrorKindClone::Empty,
            IntErrorKind::InvalidDigit => IntErrorKindClone::InvalidDigit,
            IntErrorKind::PosOverflow => IntErrorKindClone::PosOverflow,
            IntErrorKind::NegOverflow => IntErrorKindClone::NegOverflow,
            IntErrorKind::Zero => IntErrorKindClone::Zero,
            _ => defmt::unimplemented!("IntErrorKind is non-exhaustive")
        })
    }
}

impl From<fmt::Error> for CustomError {
    fn from(_: fmt::Error) -> Self {
        CustomError::FormatError
    }
}

impl From<DisplayError> for CustomError {
    fn from(err: DisplayError) -> Self {
        CustomError::DisplayError(match err {
            DisplayError::InvalidFormatError => DisplayErrorClone::InvalidFormatError,
            DisplayError::BusWriteError => DisplayErrorClone::BusWriteError,
            DisplayError::DCError => DisplayErrorClone::DCError,
            DisplayError::CSError => DisplayErrorClone::CSError,
            DisplayError::DataFormatNotImplemented => DisplayErrorClone::DataFormatNotImplemented,
            DisplayError::RSError => DisplayErrorClone::RSError,
            DisplayError::OutOfBoundsError => DisplayErrorClone::OutOfBoundsError,
            _ => defmt::unimplemented!("DisplayError is non-exhaustive")
        })
    }
}

impl From<CapacityError> for CustomError {
    fn from(_: CapacityError) -> Self {
        CustomError::CapacityError
    }
}

impl From<ReadErrorType> for CustomError {
    fn from(value: ReadErrorType) -> Self {
        CustomError::UartReadError(match value {
            ReadErrorType::Overrun => ReadErrorTypeClone::Overrun,
            ReadErrorType::Break => ReadErrorTypeClone::Break,
            ReadErrorType::Parity => ReadErrorTypeClone::Parity,
            ReadErrorType::Framing => ReadErrorTypeClone::Framing
        })
    }
}

impl From<()> for CustomError {
    fn from(_: ()) -> Self {
        CustomError::Other
    }
}