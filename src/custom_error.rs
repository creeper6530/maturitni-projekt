use core::fmt;
use defmt::Format as DefmtFormat;

use core::num::{ParseIntError, IntErrorKind, TryFromIntError};
use display_interface::DisplayError;
use heapless::CapacityError;
use rp2040_hal::uart::ReadErrorType;

// Type aliases to reduce verbosity in the From impls
// As a (self-imposed) rule, we shall not use these in fuction signatures
// or trait impls, only inside function bodies.
pub type CE = CustomError; // Public, so that it can be used elsewhere
type IEK = IntErrorKind;
type IEKC = IntErrorKindClone;
type DiE = DisplayError;
type DiEC = DisplayErrorClone;
type RET = ReadErrorType;
type RETC = ReadErrorTypeClone;

// If you're gonna move this to a library crate later,
// consider gating defmt behind a feature flag to avoid a hard dependency on it.
// Probably no need to gate core stuff nor heapless/rp2040-hal/display-interface dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DefmtFormat, Default)]
#[non_exhaustive] // So that we can add more error types later without breaking compatibility
pub enum CustomError {
    MathOverflow,
    ParseIntError(IntErrorKindClone),
    FormatError,
    BadInput,

    DisplayError(DisplayErrorClone),
    CapacityError,

    UartReadError(ReadErrorTypeClone),

    /// Like the macro - unimplemented functionality, not for an error that isn't implemented in this enum.
    /// Use the Other variant for that.
    Unimplemented,
    Impossible,
    Cancelled,

    /// A miscellaneous error that doesn't fit in any other variant.
    /// Use the OtherString variant if you want to provide more context.
    #[default] Other, // We have to mark a default variant by this attribute for the Default derive
    /// A miscellaneous error that doesn't fit in any other variant, with a string for more context.
    /// We restrict to 'static str (string literals and other comptime) to avoid lifetime issues.
    /// 
    /// If you're crazy, you could use alloc::String::shrink_to_fit and ::leak to get a 'static str at runtime.
    /// (Provided you have the alloc crate available, of course.)
    OtherStr(&'static str)
}

// Because IntErrorKind doesn't implement defmt::Format.
// If we moved this into a library crate and gated defmt,
// consider conditionally compiling this and otherwise using IntErrorKind directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DefmtFormat)]
#[non_exhaustive]
pub enum IntErrorKindClone {
    Empty,
    InvalidDigit,
    PosOverflow,
    NegOverflow,
    Zero,
}

// Because DisplayError doesn't implement PartialEq nor Eq, or at least until my PR gets merged. (It should implement defmt::Format though.)
// Said PR: https://github.com/therealprof/display-interface/pull/55
#[derive(Debug, Clone, Copy, PartialEq, Eq, DefmtFormat)]
#[non_exhaustive]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, DefmtFormat)]
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

// Happens when you try to convert bigger int into smaller and it's outside the range.
// We map it to MathOverflow for simplicity, since it's a kind of overflow.
impl From<TryFromIntError> for CustomError {
    fn from(_: TryFromIntError) -> Self {
        CE::MathOverflow
    }
}

impl From<ParseIntError> for CustomError {
    fn from(err: ParseIntError) -> Self {
        let kind = *err.kind();
        
        CE::ParseIntError(match kind {
            IEK::Empty => IEKC::Empty,
            IEK::InvalidDigit => IEKC::InvalidDigit,
            IEK::PosOverflow => IEKC::PosOverflow,
            IEK::NegOverflow => IEKC::NegOverflow,
            IEK::Zero => IEKC::Zero,
            err => unimplemented!("IntErrorKind is non-exhaustive and a new variant has been added: {:?}", err)
        })
    }
}

impl From<fmt::Error> for CustomError {
    fn from(_: fmt::Error) -> Self {
        CE::FormatError
    }
}

impl From<DisplayError> for CustomError {
    fn from(err: DisplayError) -> Self {
        CE::DisplayError(match err {
            DiE::InvalidFormatError => DiEC::InvalidFormatError,
            DiE::BusWriteError => DiEC::BusWriteError,
            DiE::DCError => DiEC::DCError,
            DiE::CSError => DiEC::CSError,
            DiE::DataFormatNotImplemented => DiEC::DataFormatNotImplemented,
            DiE::RSError => DiEC::RSError,
            DiE::OutOfBoundsError => DiEC::OutOfBoundsError,
            err => unimplemented!("DisplayError is non-exhaustive and a new variant has been added: {:?}", err)
        })
    }
}

impl From<CapacityError> for CustomError {
    fn from(_: CapacityError) -> Self {
        CE::CapacityError
    }
}

impl From<ReadErrorType> for CustomError {
    fn from(value: ReadErrorType) -> Self {
        CE::UartReadError(match value {
            RET::Overrun => RETC::Overrun,
            RET::Break => RETC::Break,
            RET::Parity => RETC::Parity,
            RET::Framing => RETC::Framing
        })
    }
}

impl From<()> for CustomError {
    fn from(_: ()) -> Self {
        CE::Other
    }
}

/// We restrict to 'static str (string literals and other comptime) to avoid lifetime issues.
/// 
/// For example, if we generated a non-'static str at runtime,
/// and then try to convert it into CustomError that we'd return from a function,
/// we'd have a dangling reference because the pointed-to str would go out of scope
/// at the end of the function.
/// 
/// The alternative would be to use String, but that would bar us from deriving Copy.
/// If you're crazy, you could use alloc::String::shrink_to_fit and ::leak to get a 'static str at runtime.
/// (Provided you have the alloc crate available, of course.
impl From<&'static str> for CustomError {
    fn from(value: &'static str) -> Self {
        CE::OtherStr(value)
    }
}