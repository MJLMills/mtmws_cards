#![cfg_attr(not(test), no_std)]

use core::fmt::Debug;
use core::ops::{Div, Mul, Shr, Sub};

use defmt::*;

/// A 12 bit value representing input from a knob or input jack's ADC
///
/// Normalized to the range -2048 to 2047 inclusive. Stored as i32 to give
/// room for integer math without needing allocations and the rp2040 is 32bit.
/// Conversions from/to this type saturate (clamp) - they stop at the min/max
/// values without giving errors. Before converting, raw internal value may be
/// outside of 12 bit range (allowing for math & accumulations, etc).
#[derive(Format, PartialEq, Copy, Clone)]
pub struct InputValue {
    accumulated_raw: i32,
    inverted_source: bool,
}

impl Debug for InputValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::write!(
            f,
            "InputValue::new({}, {})",
            self.accumulated_raw >> Self::ACCUM_BITS,
            self.inverted_source,
        )
    }
}

impl InputValue {
    // CONST for min/max values (12 bit limits, 11 on each positive/negative)
    pub const MIN: i32 = -2_i32.pow(11);
    pub const MAX: i32 = 2_i32.pow(11) - 1;
    pub const CENTER: i32 = 0;
    pub const OFFSET: i32 = 2_i32.pow(11);
    const ACCUM_BITS: u8 = 4;

    pub fn new(raw_value: i32, invert: bool) -> Self {
        InputValue {
            accumulated_raw: match invert {
                false => raw_value << Self::ACCUM_BITS,
                true => -raw_value << Self::ACCUM_BITS,
            },
            inverted_source: invert,
        }
    }

    /// Convert from u16 and offset value so center is at zero
    pub fn from_u16(value: u16, invert: bool) -> Self {
        let mut output = i32::from(value);
        output -= Self::OFFSET;
        Self::new(output, invert)
    }

    pub fn update(&mut self, value: u16) {
        let mut value = i32::from(value);
        value -= Self::OFFSET;
        if self.inverted_source {
            value -= value;
        }
        // TODO: actually smooth values here
        self.accumulated_raw = value << Self::ACCUM_BITS;
    }

    /// Saturating conversion into 11 bit safe u16 for output
    pub fn to_output(&self) -> u16 {
        // clamp self, divide by 2 (by shifting right) and convert to u16
        (self.to_clamped() + Self::OFFSET).shr(1) as u16
    }

    /// Saturating conversion into 11 bit safe u16 for output, inverted
    pub fn to_output_inverted(&self) -> u16 {
        2047_u16.saturating_sub(self.to_output())
    }

    pub fn to_clamped(&self) -> i32 {
        (self.accumulated_raw >> Self::ACCUM_BITS).clamp(Self::MIN, Self::MAX)
    }
}

impl Sub for InputValue {
    type Output = Self;

    fn sub(mut self, rhs: Self) -> Self::Output {
        self.accumulated_raw -= rhs.accumulated_raw;
        self
    }
}

impl Mul for InputValue {
    type Output = Self;

    fn mul(mut self, rhs: Self) -> Self::Output {
        self.accumulated_raw = ((self.accumulated_raw >> Self::ACCUM_BITS)
            * (rhs.accumulated_raw >> Self::ACCUM_BITS))
            << Self::ACCUM_BITS;
        self
    }
}

impl Mul<i32> for InputValue {
    type Output = Self;

    fn mul(mut self, rhs: i32) -> Self::Output {
        self.accumulated_raw =
            ((self.accumulated_raw >> Self::ACCUM_BITS) * rhs) << Self::ACCUM_BITS;
        self
    }
}

impl Div<i32> for InputValue {
    type Output = Self;

    fn div(mut self, rhs: i32) -> Self::Output {
        self.accumulated_raw =
            ((self.accumulated_raw >> Self::ACCUM_BITS) / rhs) << Self::ACCUM_BITS;
        self
    }
}

#[cfg(test)]
mod test {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::InputValue;

    #[test]
    fn test_input_value_basics() {
        assert_eq!(InputValue::MIN, -2048);
        assert_eq!(InputValue::MAX, 2047);
    }

    #[test]
    fn test_input_value_to_clamped() {
        // clamp to 12 bit values when inputs are above range
        assert_eq!(
            InputValue::from_u16(8000, false).to_clamped(),
            InputValue::MAX
        );
        assert_eq!(
            InputValue::from_u16(5000, false).to_clamped(),
            InputValue::MAX
        );
        assert_eq!(
            InputValue::from_u16(4096, false).to_clamped(),
            InputValue::MAX
        );
    }

    #[test]
    fn test_input_value_from() {
        assert_eq!(InputValue::from_u16(0, false).to_clamped(), InputValue::MIN);
        assert_eq!(InputValue::from_u16(2048, false).to_clamped(), 0);
        assert_eq!(
            InputValue::from_u16(4095, false).to_clamped(),
            InputValue::MAX
        );
    }

    #[test]
    fn test_input_value_to_output() {
        // output values are half of input (11 bit from 12 bit)
        assert_eq!(InputValue::from_u16(0, false).to_output(), 0);
        assert_eq!(InputValue::from_u16(2_u16, false).to_output(), 1_u16);
        assert_eq!(InputValue::from_u16(1024_u16, false).to_output(), 512_u16);
        assert_eq!(InputValue::from_u16(2048_u16, false).to_output(), 1024_u16);

        // clamp to 11 bit values in to_output() when inputs are above range
        assert_eq!(InputValue::from_u16(8000, false).to_output(), 2047_u16);
        assert_eq!(InputValue::from_u16(5000, false).to_output(), 2047_u16);
        assert_eq!(InputValue::from_u16(4096, false).to_output(), 2047_u16);

        let below_range = InputValue::from_u16(0, false) - InputValue::new(5000, false);
        assert_eq!(below_range.to_output(), 0_u16);
    }

    #[test]
    fn test_input_value_math() {
        assert_eq!(InputValue::new(123, false) * 1, InputValue::new(123, false));
        assert_eq!(InputValue::new(123, false) * 2, InputValue::new(246, false));
        assert_eq!(
            InputValue::new(123, false) * -1,
            InputValue::new(-123, false)
        );

        #[allow(clippy::erasing_op)]
        let expected = InputValue::new(123, false) * 0;
        assert_eq!(expected, InputValue::new(0, false));

        // division
        assert_eq!(InputValue::new(123, false) / 1, InputValue::new(123, false));
        assert_eq!(InputValue::new(240, false) / 2, InputValue::new(120, false));
        assert_eq!(
            InputValue::new(123, false) / -1,
            InputValue::new(-123, false)
        );
    }
}
