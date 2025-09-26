use defmt::Format;
#[allow(unused_imports)]
use defmt::{trace, debug, info, warn, error, panic, unreachable, unimplemented, todo};
use heapless::format;
use core::{fmt::Display, ops::{Add, Sub, Mul, Div}, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Format)]
pub struct DecimalFixed {
    value: i64, // The actual value is value * 10^exponent
    exponent: i8,
}

// FIXME: Fix trailing zeros when exponent is too big
impl Display for DecimalFixed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.exponent == 0 {
            write!(f, "{}", self.value)
        } else if self.exponent > 0 {
            write!(f, "{}{:0width$}", self.value, 0, width = self.exponent as usize)
        } else {
            let pow = 10_i128.pow(-self.exponent as u32);
            let value = self.value.abs() as i128;
            let whole_part = value / pow; // Integer division - truncates away the last digits
            let fractional_part = value % pow; // Remainder - gets the last digits, truncates earlier digits

            if self.value < 0 { write!(f, "-")?; } // Print the negative sign if needed
            write!(f, "{}", whole_part)?;

            if fractional_part == 0 { return Ok(()); }; // No need to print .0...0 , so we return early
            write!(f, ".")?;
            write!(f, "{:0width$}", fractional_part, width = (-self.exponent) as usize)?;

            Ok(())
        }
    }
}

impl FromStr for DecimalFixed {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.find('.') {
            Some(dot_index) => {
                let (whole_part_str, fractional_part_str) = s.split_at(dot_index);
                let fractional_part_str = &fractional_part_str[1..]; // Skip the dot

                let whole_part = whole_part_str.parse::<i64>().map_err(|_| ())?;
                let fractional_part = if fractional_part_str.is_empty() {
                    0
                } else {
                    fractional_part_str.parse().map_err(|_| ())?
                };

                let exponent = -(fractional_part_str.len() as i8);
                let value = whole_part * 10_i64.pow(-exponent as u32) + fractional_part;

                Ok( DecimalFixed { value, exponent } )
            }
            None => {
                Ok( DecimalFixed { value: s.parse::<i64>().map_err(|_| ())? , exponent: 0 } )
            }
        }
    }
}

impl Default for DecimalFixed {
    fn default() -> Self {
        Self { value: 0, exponent: -9 }
    }
}

impl DecimalFixed {
    pub fn new(value: i64) -> Self {
        Self { value, exponent: -9 }
    }

    pub fn custom_exp(value: i64, exponent: i8) -> Self {
        Self { value, exponent }
    }

    pub fn parse_static_exp(s: &str, exp: i8) -> Result<Self, ()> {
        match s.find('.') {
            Some(dot_index) => {
                let (whole_part_str, mut fractional_part_str) = s.split_at(dot_index);
                fractional_part_str = &fractional_part_str[1..]; // Skip the dot

                let whole_part = whole_part_str.parse::<i64>().map_err(|_| ())?;
                let fractional_part = if fractional_part_str.is_empty() {
                    0
                } else {
                    let buf_string;
                    if exp >= 0 { todo!() } // TODO: Handle this case if needed; meanwhile we just panic

                    // Transform the fractional part to be (-exp) digits long - either pad at the end or truncate
                    if fractional_part_str.len() > (-exp as usize) { // Truncate
                        fractional_part_str = &fractional_part_str[..(-exp as usize)];

                        // Sanity check - this should always be true
                        debug_assert_eq!(fractional_part_str.len(), -exp as usize);
                    } else if fractional_part_str.len() < (-exp as usize) { // Pad
                        buf_string = format!(20; "{:0<width$}", fractional_part_str, width = (-exp) as usize).map_err(|_| ())?;
                        fractional_part_str = buf_string.as_str();

                        debug_assert_eq!(fractional_part_str.len(), -exp as usize);
                    } /*else {
                        // do nothing
                    }*/

                    fractional_part_str.parse().map_err(|_| ())?
                };

                let value = whole_part * 10_i64.pow(-exp as u32) + fractional_part;

                Ok( DecimalFixed { value, exponent: exp } )
            }
            None => {
                let base_num = s.parse::<i64>().map_err(|_| ())?;
                if exp < 0 {
                    Ok( DecimalFixed { value: base_num.checked_mul(10_i64.pow((-exp) as u32)).ok_or(())? , exponent: exp } )
                } else {
                    Ok( DecimalFixed { value: base_num / 10_i64.pow(exp as u32) , exponent: exp } )
                }
            }
        }
    }

    fn priv_add(&self, other: DecimalFixed) -> DecimalFixed {
        if self.exponent == other.exponent {
            DecimalFixed { value: self.value + other.value, exponent: self.exponent }
        } else {
            let exp_diff = (self.exponent - other.exponent) as u32;
            if self.exponent > other.exponent {
                DecimalFixed { value: self.value * 10_i64.pow(exp_diff) + other.value , exponent: other.exponent }
            } else {
                DecimalFixed { value: self.value + other.value * 10_i64.pow(exp_diff) , exponent: self.exponent }
            }
        }
    }

    fn priv_sub(&self, other: DecimalFixed) -> DecimalFixed {
        if self.exponent == other.exponent {
            DecimalFixed { value: self.value - other.value, exponent: self.exponent }
        } else {
            let exp_diff = (self.exponent - other.exponent) as u32;
            if self.exponent > other.exponent {
                DecimalFixed { value: self.value * 10_i64.pow(exp_diff) - other.value , exponent: other.exponent }
            } else {
                DecimalFixed { value: self.value - other.value * 10_i64.pow(exp_diff) , exponent: self.exponent }
            }
        }
    }
    
    fn priv_mul(&self, other: DecimalFixed, keep_exponent: bool) -> Result<DecimalFixed, ()> {
        // Multiplying two fixed-point numbers:
        // (value1 * 10^exp1) * (value2 * 10^exp2) = (value1 * value2) * 10^(exp1 + exp2)
        if keep_exponent {
            if self.exponent != other.exponent { return Err(()) }
            
            let scaled_end_value: i128 = i128::from(self.value) * i128::from(other.value); // Due to the scaling (addition of exponents), the value can get very large, so we use i128 here
            let end_value: i128 = if self.exponent < 0 {
                scaled_end_value / 10_i128.pow((-self.exponent) as u32) // After downscaling back, it should hopefully fit in i64 again
            } else {
                scaled_end_value.checked_mul(10_i128.pow(self.exponent as u32)).ok_or(())?
            };

            if end_value < i128::from(i64::MIN) || end_value > i128::from(i64::MAX) { return Err(()) };

            Ok( DecimalFixed { value: i64::try_from(end_value).unwrap() , exponent: self.exponent } ) // Should be safe to unwrap thanks to the check above
        } else {
            Ok( DecimalFixed { value: self.value.checked_mul(other.value).ok_or(())? , exponent: self.exponent + other.exponent } )
        }
    }

    fn priv_div(&self, other: DecimalFixed, keep_exponent: bool) -> Result<DecimalFixed, ()> {
        // Dividing two fixed-point numbers:
        // (value1 * 10^exp1) / (value2 * 10^exp2) = (value1 / value2) * 10^(exp1 - exp2)
        if keep_exponent {
            if self.exponent != other.exponent { return Err(()) }

            // We double the exponent in the numerator to keep it the same after division
            if other.value == 0 { return Err(()) }; // Division by zero check

            let scaled_self_value: i128 = if self.exponent < 0 {
                i128::from(self.value).checked_mul(10_i128.pow((-self.exponent) as u32)).ok_or(())?
            } else {
                i128::from(self.value) / 10_i128.pow(self.exponent as u32)
            };
            let end_value: i128 = scaled_self_value / i128::from(other.value);

            if end_value < i128::from(i64::MIN) || end_value > i128::from(i64::MAX) { return Err(()) };
            Ok( DecimalFixed { value: i64::try_from(end_value).unwrap() , exponent: self.exponent } )
        } else {
            Ok( DecimalFixed { value: self.value / other.value , exponent: self.exponent - other.exponent } )
        }
    }

    /// Unlike the `*` operator, this keeps the exponent the same
    pub fn multiply(&self, other: DecimalFixed) -> Result<DecimalFixed, ()> {
        self.priv_mul(other, true)
    }

    /// Unlike the `/` operator, this keeps the exponent the same
    pub fn divide(&self, other: DecimalFixed) -> Result<DecimalFixed, ()> {
        self.priv_div(other, true)
    }

    pub fn negate(&self) -> Result<DecimalFixed, ()> {
        if self.value == i64::MIN { return Err(()) } // Negating i64::MIN would overflow
        Ok ( DecimalFixed { value: -self.value, exponent: self.exponent } )
    }

    pub fn negate_in_place(&mut self) -> () {
        self.value = -self.value;
    }

    pub fn is_negative(&self) -> bool {
        self.value < 0
    }

    pub fn is_zero(&self) -> bool {
        self.value == 0
    }
}

// Apparently it is idiomatic to implement `From` instead of `Into`, because `Into` is automatically implemented
impl From<DecimalFixed> for i64 {
    fn from(input: DecimalFixed) -> Self {
        if input.exponent == 0 {
            input.value
        } else if input.exponent > 0 {
            input.value * 10_i64.pow(input.exponent as u32)
        } else {
            // This will truncate the decimal part, which is expected when converting to integer
            input.value / 10_i64.pow((-input.exponent) as u32)
        }
    }
}

impl From<DecimalFixed> for f64 {
    /// Almost certainly a lossy conversion
    fn from(input: DecimalFixed) -> Self {
        if input.exponent == 0 {
            input.value as f64
        } else if input.exponent > 0 {
            input.value as f64 * (10_i64.pow(input.exponent as u32)) as f64   
        } else {
            // This should hopefully not lose the decimal part
            // TODO: Test this
            input.value as f64 / (10_i64.pow((-input.exponent) as u32)) as f64
        }
    }
}

impl Add for DecimalFixed {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        self.priv_add(other)
    }
}

impl Sub for DecimalFixed {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        self.priv_sub(other)
    }
}

impl Mul for DecimalFixed {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        self.priv_mul(other, false).unwrap()
    }
}

impl Div for DecimalFixed {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        self.priv_div(other, false).unwrap()
    }
}