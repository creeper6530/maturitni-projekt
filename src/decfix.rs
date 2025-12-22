use defmt::Format as DefmtFormat;
use heapless::String;
use core::{
    fmt::Display,
    ops::{Add, Sub, Neg, Mul, Div},
    str::FromStr,
    cmp::Ordering
};

use crate::custom_error::CustomError; // Because we already have the `mod` in `main.rs`
use CustomError as CE; // Short alias for easier use

const DEFAULT_EXPONENT: i8 = -9;
const PARSING_BUFFER_SIZE: usize = 16; // Buffer size for padding fractional parts when parsing strings

/// A fixed-point decimal number with a variable exponent.
/// Has basic arithmetic operations implemented, as well as parsing from string and formatting to string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DefmtFormat)]
pub struct DecimalFixed {
    value: i64, // The actual value is value * 10^exponent
    exponent: i8,
}

impl Display for DecimalFixed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.value == 0 {
            return write!(f, "0");
        }
        
        match self.exponent.cmp(&0) {
            Ordering::Equal => {
                write!(f, "{}", self.value)?;
            },
            Ordering::Greater => {
                // Write the value, then the trailing zeroes repeated `self.exponent` times
                write!(f, "{}{:0>width$}", self.value, "", width = self.exponent as usize)?;

                /* Explanation:
                {value} - Format the first positional argument (self.value) Display style
                {:0>width$} - Format the second positional argument (empty string) Display style,
                    with padding character '0' = a zero,
                    right-aligned with minimum width of the `width` variable (self.exponent).
                    └─> Repeats a zero `width` times
                
                It may be tempting to use 10.pow(self.exponent), but that would add a '1' between the numbers and the zeroes, which we don't want.
                *annoyed sigh* Ask me how I know. */
            },
            Ordering::Less => 'exit_match: { // Declaring a labelled block with the label 'exit_match
                // Equal and Greater cases add their own negative signs
                if self.value.is_negative() {
                    write!(f, "-")?;
                }

                let value = self.value.abs();
                let pow = 10_i64.pow((-self.exponent) as u32);

                let whole_part = value / pow; // Integer division by power of ten truncates away last digits
                let mut fractional_part = value % pow; // Integer modulo by power of ten gets the discarded last digits back

                write!(f, "{}", whole_part)?;
                if fractional_part == 0 { break 'exit_match } // Since we're in a labelled block, we can short-circuit to its end

                let mut width = (-self.exponent) as usize;
                while fractional_part % 10 == 0 {
                    fractional_part /= 10; // Remove trailing zeroes from the fractional part for cleaner display
                    width -= 1; // Decrease the width accordingly so that we don't turn 3.1400 into 3.0014
                }

                // We still need to pad with zeroes, aligning right, if we had e.g. 3.0014
                // The parameter `width` can take from a variable too btw
                write!(f, ".{:0>width$}", fractional_part)?;
            }
        }

        Ok(())
    }
}

impl FromStr for DecimalFixed {
    type Err = CustomError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.find('.') {
            Some(dot_index) => {
                let (whole_part_str, fractional_part_str) = s.split_at(dot_index);
                let fractional_part_str = &fractional_part_str[1..]; // Skip the dot

                let whole_part = whole_part_str.parse::<i64>()?;
                let fractional_part: i64 = if fractional_part_str.is_empty() {
                    0
                } else {
                    fractional_part_str.parse()?
                };
                let exponent = -(fractional_part_str.len() as i8);

                let mut value = whole_part.checked_mul(
                    10_i64.pow(-exponent as u32)
                ).ok_or(CE::MathOverflow)?;

                value = value.checked_add(
                    fractional_part
                ).ok_or(CE::MathOverflow)?;

                Ok( DecimalFixed { value, exponent } )
            }
            None => {
                Ok( DecimalFixed { value: s.parse::<i64>()? , exponent: 0 } )
            }
        }
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
    pub fn new(value: i64, exponent: Option<i8>) -> Result<Self, CustomError> {
        let exponent = exponent.unwrap_or(DEFAULT_EXPONENT);
        
        match exponent.cmp(&0) {
            Ordering::Equal => {
                Ok( Self { value, exponent } )
            },
            Ordering::Greater => {
                // Scaling down - dividing value by 10^exponent
                let scaled_value = value / 10_i64.pow(exponent as u32);

                Ok( Self { value: scaled_value, exponent } )
            },
            Ordering::Less => {
                // Scaling up - dividing value by 10^(-exponent) - multiplying by 10^(exponent) to stay in integers
                let scaled_value = value.checked_mul(
                    10_i64.pow((-exponent) as u32)
                ).ok_or(CE::MathOverflow)?;

                Ok( Self { value: scaled_value, exponent } )
            }
        }
    }

    /// Creates a new DecimalFixed with the given value and exponent, without any scaling.
    /// Please ensure that the value you provide is already scaled correctly.
    pub fn new_prescaled(value: i64, exponent: i8) -> Self {
        Self { value, exponent }
    }

    /// Parses a string into a DecimalFixed with the exponent you provide,
    /// or the default exponent specified in a const if you pass None.
    /// If the string has a fractional part that isn't the correct size, it will be truncated/padded to fit the exponent.
    /// 
    /// If you want to parse a string and let the exponent adjust dynamically to your input, use `str::parse::<DecimalFixed>()` instead.
    pub fn parse_static_exp(s: &str, exp: Option<i8>) -> Result<Self, CustomError> {
        let exp = exp.unwrap_or(DEFAULT_EXPONENT);

        if exp >= 0 { return Err(CE::Unimplemented) }; // TODO: Handle this case if needed
        if s.is_empty() { return Err( CE::BadInput ) };
        let minus_exp = -exp as usize;

        let mut iter = s.splitn(2, '.'); // Split into at most two parts, at the first dot

        let whole_part: &str = iter.next().expect("First .next() on SplitN should be Some!");
        let whole_part: i64 = whole_part.parse::<i64>()?;

        let mut value = whole_part.checked_mul(
            10_i64.pow(minus_exp as u32)
        ).ok_or(CE::MathOverflow)?;

        let frac_part_option = iter.next();
        if frac_part_option.is_some_and(|n| { !n.is_empty() }) {
            let frac_part: &str = frac_part_option.unwrap(); // Safe to unwrap thanks to is_some_and check

            let mut buf_string; // Declare uninitialized here so that it lives long enough
            let processed = match frac_part.len().cmp(&minus_exp) {
                Ordering::Equal => frac_part,
                Ordering::Greater => &frac_part[..(minus_exp)], // Truncate
                Ordering::Less => { // Pad with zeroes
                    // So far have not found a way to do this without a String, since we need it to be mutable
                    // Using the `format!` macro would increase code size by up to one KiB (checked with `cargo size`), so we use push instead

                    // Perhaps create a number right in the `match` expression and multiply by ten?
                    buf_string = String::<PARSING_BUFFER_SIZE>::from_str(frac_part)?;

                    for _ in 0..(minus_exp - frac_part.len()) {
                        buf_string.push('0')?;
                    }
                    buf_string.as_str()
                }
            };
            // Sanity check - this should always be true
            defmt::debug_assert_eq!(processed.len(), minus_exp);

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

        Ok( DecimalFixed { value, exponent: exp } )
    }

    /// Unlike the `+` operator, this returns a Result instead of panicking on overflow
    pub fn addition(&self, other: DecimalFixed) -> Result<DecimalFixed, CustomError> {
        self.priv_add(other)
    }

    /// Unlike the `-` operator, this returns a Result instead of panicking on overflow
    pub fn subtract(&self, mut other: DecimalFixed) -> Result<DecimalFixed, CustomError> {
        // We don't implement a separate `priv_sub()` so that we don't duplicate the code for what's essentially the same operation
        other.negate_in_place()?;
        self.priv_add(other)
    }

    /// Returns a new DecimalFixed, which is the negation of self
    pub fn negate(&self) -> Result<DecimalFixed, CustomError> {
        if self.is_zero() { return Ok( *self ) }; // Negating zero is still zero
        if self.value == i64::MIN { return Err( CE::MathOverflow ) }; // Negating i64::MIN would overflow
        Ok ( DecimalFixed { value: -self.value, exponent: self.exponent } )
    }

    /// Negates self in place, modifying the original value and saving a bit of memory.
    /// This of course needs a mutable reference to self.
    pub fn negate_in_place(&mut self) -> Result<(), CustomError> {
        if self.is_zero() { return Ok(()) };
        if self.value == i64::MIN { return Err( CE::MathOverflow ) };
        self.value = -self.value;
        Ok(())
    }

    /// Unlike the `*` operator, this keeps the exponent the same
    pub fn multiply(&self, other: DecimalFixed) -> Result<DecimalFixed, CustomError> {
        self.priv_mul(other, true)
    }

    /// Like the `*` operator, this changes the exponent,
    /// but returns a Result instead of unwrapping.
    pub fn multiply_no_keep_exp(&self, other: DecimalFixed) -> Result<DecimalFixed, CustomError> {
        self.priv_mul(other, false)
    }

    /// Unlike the `/` operator, this keeps the exponent the same
    pub fn divide(&self, other: DecimalFixed) -> Result<DecimalFixed, CustomError> {
        self.priv_div(other, true)
    }

    /// Like the `/` operator, this changes the exponent,
    /// but returns a Result instead of unwrapping.
    pub fn divide_no_keep_exp(&self, other: DecimalFixed) -> Result<DecimalFixed, CustomError> {
        self.priv_div(other, false)
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

// For private methods - to separate the blocks of code
impl DecimalFixed {
    fn priv_add(&self, other: DecimalFixed) -> Result<DecimalFixed, CustomError> {
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
                    10_i64.pow((self.exponent - other.exponent) as u32)
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
                    10_i64.pow((self.exponent - other.exponent) as u32)
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
    
    fn priv_mul(&self, other: DecimalFixed, keep_exponent: bool) -> Result<DecimalFixed, CustomError> {
        // Multiplying two fixed-point numbers:
        // (value1 * 10^exp1) * (value2 * 10^exp2) = (value1 * value2) * 10^(exp1 + exp2)

        if !keep_exponent {
            return Ok( DecimalFixed{
                value: self.value.checked_mul(other.value).ok_or(CE::MathOverflow)?,
                exponent: self.exponent.checked_add(other.exponent).ok_or(CE::MathOverflow)?
            })
        }

        // From now on, operate under the assumption that keep_exponent == true (because we diverged above)
        if self.exponent != other.exponent { return Err( CE::Unimplemented ) }

        // Due to the scaling (addition of exponents), the value can get very large, so we use i128 here
        let scaled_end_value: i128 = i128::from(self.value)
            .checked_mul(
                i128::from(other.value)
            ).ok_or(CE::MathOverflow)?;

        // We do 10_i64 so that we don't need 4.4KiB of i128::pow()
        // Yes, it's silly to do microoptimisation in this project, but I enjoy it in some twisted way.
        let scale_factor: i128 = i128::from(10_i64.pow(self.exponent.abs() as u32));
        let end_value: i128 = if self.exponent >= 0 {
            scaled_end_value.checked_mul(scale_factor).ok_or(CE::MathOverflow)?
        } else {
            // Division can only overflow if we divide INT_MIN by -1, which is impossible here since 10^x is never -1, so we don't check for it
            scaled_end_value / scale_factor
        };

        Ok( DecimalFixed { value: i64::try_from(end_value)? , exponent: self.exponent } )
    }

    fn priv_div(&self, other: DecimalFixed, keep_exponent: bool) -> Result<DecimalFixed, CustomError> {
        // Dividing two fixed-point numbers:
        // (value1 * 10^exp1) / (value2 * 10^exp2) = (value1 / value2) * 10^(exp1 - exp2)

        if other.value == 0 { return Err( CE::BadInput ) }; // Division by zero check

        if !keep_exponent {
            return Ok( DecimalFixed{
                value: self.value / other.value,
                exponent: self.exponent.checked_sub(other.exponent).ok_or(CE::MathOverflow)?
            })
        }

        // From now on, operate under the assumption that keep_exponent == true (because we diverged above)
        if self.exponent != other.exponent { return Err( CE::Unimplemented ) }

        // We do 10_i64 so that we don't need 4.4KiB of i128::pow()
        // Yes, it's silly to do microoptimisation in this project.
        let scale_factor: i128 = i128::from(10_i64.pow(self.exponent.abs() as u32));
        let scaled_self_value: i128 = if self.exponent >= 0 {
            i128::from(self.value) / scale_factor
        } else {
            i128::from(self.value).checked_mul(scale_factor).ok_or(CE::MathOverflow)?
        };

        let end_value: i128 = scaled_self_value / i128::from(other.value);

        Ok( DecimalFixed { value: i64::try_from(end_value)? , exponent: self.exponent } )
    }
}

impl Add for DecimalFixed {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output { // In this case we could just return `Self`, but it's better to be consistent with how others implement this family of traits
        self.priv_add(other).unwrap()
    }
}

impl Sub for DecimalFixed {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        // We don't implement a separate `priv_sub()` so that we don't duplicate the code for what's essentially the same operation
        self.priv_add(other.negate().unwrap()).unwrap()
    }
}

impl Neg for DecimalFixed {
    type Output = Self;

    fn neg(mut self) -> Self::Output { // We're taking self by value, not by reference, so we can modify it in place and save a bit of memory
        self.negate_in_place().unwrap();
        self
    }
}

impl Mul for DecimalFixed {
    type Output = Self;

    fn mul(self, other: Self) -> Self::Output {
        self.priv_mul(other, false).unwrap()
    }
}

impl Div for DecimalFixed {
    type Output = Self;

    fn div(self, other: Self) -> Self::Output {
        self.priv_div(other, false).unwrap()
    }
}