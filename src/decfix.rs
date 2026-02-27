use defmt::Format as DefmtFormat;
use heapless::{String, format};
use core::{
    fmt::Display,
    ops::{Add, Sub, Neg, Mul, Div},
    str::FromStr,
    cmp::Ordering
};

use crate::custom_error::{ // Because we already have the `mod` in `main.rs`
    CustomError,
    CE // Short type alias
};

const DEFAULT_EXPONENT: i32 = -9;
const PARSING_BUFFER_SIZE: usize = 32; // Buffer size for padding fractional parts when parsing strings and displaying them.

#[derive(Debug, Clone, Copy, PartialEq, Eq, DefmtFormat)]
pub struct DecimalFixed {
    value: i64, // The actual, logical value is (value * 10^exponent)
    exponent: i32,
}

impl Display for DecimalFixed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.value == 0 {
            return write!(f, "0");
        }
        
        // By convention, the enum value means `self <operator> zero`
        match self.exponent.cmp(&0) {
            Ordering::Equal => {
                write!(f, "{}", self.value)?;
            },
            Ordering::Greater => {
                // The parameter `width` can (and here will) be taken from a local variable
                // No idea why we need to cast width to usize here, but the macro demands it
                // The unsigned_abs() is basically a safe way to get the absolute value of an i32 as u32
                let width = self.exponent.unsigned_abs() as usize;

                // Write the value, then the trailing zeroes repeated `self.exponent` times
                // No idea why we need to cast width to usize here
                write!(f, "{}{:0>width$}", self.value, "")?;

                /* Explanation:
                {} - Format the first positional argument (self.value) Display style
                {:0>width$} - Format the second positional argument (empty string) Display style,
                    right-aligned with minimum width of the `width` variable,
                    with padding character '0' = a zero
                    └─> Repeats a zero `width` times */
            },
            Ordering::Less => {
                if self.value.is_negative() {
                    write!(f, "-")?;
                }
                let width = self.exponent.unsigned_abs() as usize;
                
                // Pad the value with leading zeroes if it's too short
                let unsplit_str: String<PARSING_BUFFER_SIZE> = format!("{:0>width$}", self.value.unsigned_abs(), width = width + 1)?;
                    // +1 because we need to split the string into whole and fractional part,
                    // and the whole part needs to be at least 1 digit long (even if it's just zero),
                    // so the unsplit string needs to be at least `width + 1` characters long

                // We make the (reasonable) assumption that it's all ASCII
                let (whole_part, frac_part) = unsplit_str.split_at(
                    (unsplit_str.len() as usize) - width
                );
                let frac_part = frac_part.trim_end_matches('0');
                
                write!(f, "{}", whole_part)?;
                if !frac_part.is_empty() {
                    write!(f, ".{}", frac_part)?;
                }
            }
        }

        Ok(())
    }
}

impl Default for DecimalFixed {
    fn default() -> Self {
        Self { value: 0, exponent: DEFAULT_EXPONENT }
    }
}

impl DecimalFixed {
    /// Creates a new DecimalFixed with the given value and exponent.
    /// This function scales your input value accordingly.
    /// 
    /// Pass None as exponent to use the default exponent defined by a const.
    /// Use new_prescaled() if you already have the scaled value.
    pub fn new(value: i64, exponent: Option<i32>) -> Result<Self, CustomError> {
        let exponent = exponent.unwrap_or(DEFAULT_EXPONENT);
        
        match exponent.cmp(&0) {
            Ordering::Equal => {
                Ok( Self { value, exponent } )
            },
            Ordering::Greater => {
                // Scaling down - dividing value by 10^exponent
                let scaled_value = value / 10_i64.pow(exponent.unsigned_abs());

                Ok( Self { value: scaled_value, exponent } )
            },
            Ordering::Less => {
                // Scaling up - dividing value by 10^(-exponent) - multiplying by 10^(exponent) to stay in integers
                let scaled_value = value.checked_mul(
                    10_i64.pow(exponent.unsigned_abs())
                ).ok_or(CE::MathOverflow)?;

                Ok( Self { value: scaled_value, exponent } )
            }
        }
    }

    /// Creates a new DecimalFixed with the given value and exponent, without any scaling.
    /// Please ensure that the value you provide is already scaled correctly, otherwise, use new().
    pub fn new_prescaled(value: i64, exponent: i32) -> Self {
        Self { value, exponent }
    }

    /// Parses a string into a DecimalFixed with the exponent you provide,
    /// or the default exponent specified in a const if you pass None.
    /// 
    /// If the string has a fractional part that isn't the correct size, it will be truncated/padded to fit the exponent.
    pub fn parse_str(s: &str, exp: Option<i32>) -> Result<Self, CustomError> {
        let exp = exp.unwrap_or(DEFAULT_EXPONENT);
        if exp >= 0 { return Err(CE::Unimplemented) }; // TODO: Handle this case if needed
        let minus_exp = exp.unsigned_abs(); // Changes type to unsigned

        if s.is_empty() { return Err( CE::BadInput ) };

        let mut iter = s.splitn(2, '.'); // Split into at most two parts, at the first dot from left

        let whole_part_str: &str = iter.next().expect("First .next() on SplitN should be Some!");
        let whole_part = whole_part_str.parse::<i64>()?;

        let mut value = whole_part.checked_mul(
            10_i64.pow(minus_exp)
        ).ok_or(CE::MathOverflow)?;

        let frac_part_option = iter.next();
        if frac_part_option.is_some_and(|n| { !n.is_empty() }) {
            let frac_part: &str = frac_part_option.expect("We already checked frac_part_option to be Some!");

            // Declare uninitialized here so that it lives long enough
            // (because `processed` references it)
            let mut buf_string;
            let processed: &str = match frac_part.len().cmp(&(minus_exp as usize)) {
                Ordering::Equal => frac_part,
                Ordering::Greater => &frac_part[..(minus_exp as usize)], // Truncate
                Ordering::Less => { // Pad with zeroes
                    // So far have not found a way to do this without a String, since we need it to be mutable

                    // We could use format macro (less readable tho):
                    //buf_string = format!(20; "{:0<width$}", fractional_part_str, width = minus_exp)?;
                    buf_string = String::<PARSING_BUFFER_SIZE>::from_str(frac_part)?;

                    for _ in 0..(minus_exp as usize - frac_part.len()) {
                        buf_string.push('0')?;
                    }
                    buf_string.as_str()
                }
            };

            // Don't forget to correct for parsing negative numbers
            if value >= 0 {
                value = value.checked_add(
                    processed.parse::<i64>()?
                ).ok_or(CE::MathOverflow)?;
            } else {
                value = value.checked_sub(
                    processed.parse::<i64>()?
                ).ok_or(CE::MathOverflow)?;
            }
        };

        // Because the negative would be lost, so "-0.1" would be parsed as "0.1"
        if whole_part_str == "-0" {
            value = value.checked_neg().ok_or(CE::Impossible)?; // Negating i64::MIN would overflow, but that's not possible here since the whole part is zero
        }

        Ok( DecimalFixed { value, exponent: exp } )
    }

    /// Returns a bool as to whether the number is negative
    pub fn is_negative(&self) -> bool {
        self.value < 0
    }

    /// Returns a bool as to whether the number is zero
    pub fn is_zero(&self) -> bool {
        self.value == 0
    }
}

impl Add for DecimalFixed {
    type Output = Result<Self, CustomError>;

    fn add(self, other: Self) -> Self::Output {
        match self.exponent.cmp(&other.exponent) {
            Ordering::Equal => {
                Ok( DecimalFixed{
                    value: self.value.checked_add(
                        other.value
                    ).ok_or(CE::MathOverflow)?,
                    exponent: self.exponent
                })
            },
            Ordering::Greater => {
                let adjusted_self_value = self.value.checked_mul(
                    10_i64.pow((self.exponent - other.exponent).unsigned_abs())
                ).ok_or(CE::MathOverflow)?;

                Ok( DecimalFixed{ 
                    value: adjusted_self_value.checked_add(
                        other.value
                    ).ok_or(CE::MathOverflow)? ,
                    exponent: other.exponent
                })
            },
            Ordering::Less => {
                let adjusted_other_value = other.value.checked_mul(
                    10_i64.pow((self.exponent - other.exponent).unsigned_abs())
                ).ok_or(CE::MathOverflow)?;

                Ok( DecimalFixed{
                    value: self.value.checked_add(
                        adjusted_other_value
                    ).ok_or(CE::MathOverflow)? ,
                    exponent: self.exponent
                })
            }
        }
    }
}

impl Sub for DecimalFixed {
    type Output = Result<Self, CustomError>;

    fn sub(self, other: Self) -> Self::Output {
        // We don't duplicate the code for what's essentially the same operation
        self.add(other.neg()?)
    }
}

impl Neg for DecimalFixed {
    type Output = Result<Self, CustomError>;

    fn neg(self) -> Self::Output {
        if self.is_zero() { return Ok(self) }; // Negating zero is still zero
        let neg_value = self.value.checked_neg().ok_or(CE::MathOverflow)?; // Negating i64::MIN would overflow
        Ok ( DecimalFixed { value: neg_value, exponent: self.exponent } )
    }
}

impl Mul for DecimalFixed {
    type Output = Result<Self, CustomError>;

    fn mul(self, other: Self) -> Self::Output {
        // Multiplying two fixed-point numbers without corrections:
        // (value1 * 10^exp1) * (value2 * 10^exp2) = (value1 * value2) * 10^(exp1 + exp2)
        // That can lead into errors and unexpected shit, so we do the corrections.

        // From now on, operate under the assumption that keep_exponent == true (because we diverged above)
        if self.exponent != other.exponent { return Err( CE::Unimplemented ) }

        // Due to the scaling (addition of exponents), the value can get very large, so we use i128 here
        let scaled_end_value: i128 = i128::from(self.value)
            .checked_mul(
                i128::from(other.value)
            ).ok_or(CE::MathOverflow)?;

        // We do 10_i64 so that we don't need 4.4KiB of i128::pow()
        // Yes, it's silly to do microoptimisation in this project, but I enjoy it in some twisted way.
        let scale_factor: i128 = i128::from(10_i64.pow(self.exponent.unsigned_abs()));
        let end_value: i128 = if self.exponent >= 0 {
            scaled_end_value.checked_mul(scale_factor).ok_or(CE::MathOverflow)?
        } else {
            // Division can only overflow if we divide INT_MIN by -1, which is impossible here since 10^x is never -1, so we don't check for it
            scaled_end_value / scale_factor
        };

        Ok( DecimalFixed { value: i64::try_from(end_value)? , exponent: self.exponent } )
    }
}

impl Div for DecimalFixed {
    type Output = Result<Self, CustomError>;

    fn div(self, other: Self) -> Self::Output {
        // Dividing two fixed-point numbers without corrections:
        // (value1 * 10^exp1) / (value2 * 10^exp2) = (value1 / value2) * 10^(exp1 - exp2)
        // That can lead into errors and unexpected shit, so we do the corrections.

        if other.value == 0 { return Err( CE::BadInput ) }; // Division by zero check

        // From now on, operate under the assumption that keep_exponent == true (because we diverged above)
        if self.exponent != other.exponent { return Err( CE::Unimplemented ) }

        // We do 10_i64 so that we don't need 4.4KiB of i128::pow()
        // Yes, it's silly.
        let scale_factor: i128 = i128::from(10_i64.pow(self.exponent.unsigned_abs()));
        let scaled_self_value: i128 = if self.exponent >= 0 {
            i128::from(self.value) / scale_factor
        } else {
            i128::from(self.value).checked_mul(scale_factor).ok_or(CE::MathOverflow)?
        };

        let end_value: i128 = scaled_self_value / i128::from(other.value);

        Ok( DecimalFixed { value: i64::try_from(end_value)? , exponent: self.exponent } )
    }
}