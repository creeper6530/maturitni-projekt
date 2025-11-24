// Imports for stuff to work
#![allow(unused_imports)]
use embedded_graphics::{
    prelude::*,
    pixelcolor::BinaryColor,

    mono_font::{
//        ascii::FONT_6X12,
        iso_8859_2::FONT_6X12 as ISO_FONT_6X12,
        MonoTextStyle,
        MonoTextStyleBuilder
    },
    text::{
        Baseline,
        Text,
    },

    primitives::{
        PrimitiveStyle,
        PrimitiveStyleBuilder,
        Rectangle,
    },
};
use ssd1306::{
    Ssd1306,
    prelude::*,
    mode::BufferedGraphicsMode,
};

// Imports for the actual code
use heapless::{Vec, String};
use core::{
    prelude::v1::*, // I sincerely hope this is unnecessary, but who knows?
    cell::RefCell, // For the `RefCell` type
    cmp::min, // For the `min` function
    ops::DerefMut, // For the `deref_mut` method
    fmt::Write, // For the `write!` macro
};

// Debugging imports
use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use crate::custom_error::CustomError; // Because we already have the `mod` in `main.rs`

// ------------------------------------------------------------------------------------------------------------------------------------------------

// Note: these constants are copied in `stack.rs` as well, maintain consistency between the two files!

// Compile time constants
/** The fonts we use usually have unused pixels at the top that'd waste space,
so with this constant we basically cut off the top `n` pixels. */
const PIXELS_REMOVED: u8 = 2;
/// Size of String-s used for buffering text during writes, and for the textbox
const TEXT_BUFFER_SIZE: usize = 32;
/** Number of pixels to offset the textbox from the bottom of the display by

This constant shall be determined by the programmer,
as we won't know the font size at compile time,
and going off of defaults beats the point of the ability to change the defaults. */
const TEXTBOX_OFFSET: u8 = 4;
/** Whether to draw a cursor under the text of the textbox

May only be true of we give it the space with the TEXTBOX_OFFSET const
-- if the const is larger than one */
const TEXTBOX_CURSOR: bool = true;

// Evaluated at compile time to ensure that the constants are valid
const fn _check_consts() {
    if TEXTBOX_CURSOR && TEXTBOX_OFFSET < 1 {
        core::panic!("TEXTBOX_CURSOR can only be true if TEXTBOX_OFFSET is larger than 1");
    }
}
const _: () = _check_consts();

// ------------------------------------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayDimensions {
    pub width: u8,
    pub height: u8,
}

impl From<(u8, u8)> for DisplayDimensions {
    fn from(dimensions: (u8, u8)) -> Self {
        return DisplayDimensions {
            width: dimensions.0,
            height: dimensions.1,
        };
    }
}

impl Default for DisplayDimensions {
    fn default() -> Self {
        return DisplayDimensions {
            width: 128,
            height: 64,
        };
    }
}

// ------------------------------------------------------------------------------------------------------------------------------------------------

pub struct CustomTextboxBuilder<'a, DI, SIZE>
where
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    text: String<TEXT_BUFFER_SIZE>,

    display_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,
    disp_dimensions: DisplayDimensions,

    text_style: MonoTextStyle<'a, BinaryColor>,
    primitives_style: PrimitiveStyle<BinaryColor>,
    primitives_alternate_style: PrimitiveStyle<BinaryColor>,
}

#[allow(dead_code)]
impl<'a, DI, SIZE> CustomTextboxBuilder<'a, DI, SIZE>
where 
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    /// Creates a new `CustomTextboxBuilder` with the given display RefCell.
    /// 
    /// This constructor uses the default display dimensions of 128x64 pixels and the default text style.
    /// For custom parameters, use [`Self::new_custom()`].
    pub fn new(
        display_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>
    ) -> Self {
        return CustomTextboxBuilder {
            text: String::new(),
            
            disp_dimensions: DisplayDimensions::default(),
            display_refcell,

            // Standard white text on transparent background
            text_style: MonoTextStyleBuilder::new()
                .font(&ISO_FONT_6X12)
                .text_color(BinaryColor::On)
                //.reset_background_color() // Reset the background color to transparent (unnecessary, but for clarity)
                .build(),

            // Standard white stroke with 1px width and transparent fill
            primitives_style: PrimitiveStyleBuilder::new()
                .stroke_width(1)
                .stroke_color(BinaryColor::On)
                //.reset_fill_color() // Reset the fill color to transparent (unnecessary, but for clarity)
                .build(),

            // Standard black stroke with 1px width and black fill
            primitives_alternate_style: PrimitiveStyleBuilder::new()
                .stroke_width(1)
                .stroke_color(BinaryColor::Off)
                .fill_color(BinaryColor::Off)
                .build(),
        };
    }

    pub fn build(self) -> CustomTextbox<'a, DI, SIZE> {
        return CustomTextbox {
            text: self.text,

            disp_dimensions: self.disp_dimensions,
            display_refcell: self.display_refcell,

            text_style: self.text_style,
            primitives_style: self.primitives_style,
            primitives_alternate_style: self.primitives_alternate_style,

            debug: false,
        };
    }

    pub fn build_debug(mut self) -> CustomTextbox<'a, DI, SIZE> {
        warn!("Building a debug textbox, filling it with default values.");

        self.text.clear();
        self.text.push_str("DEBUG TEXTBOX").expect("TEXT_BUFFER_SIZE is too small for the debug message!");

        return CustomTextbox {
            // Fill the text with a debug message
            text: self.text,

            disp_dimensions: self.disp_dimensions,
            display_refcell: self.display_refcell,

            text_style: self.text_style,
            primitives_style: self.primitives_style,
            primitives_alternate_style: self.primitives_alternate_style,

            debug: true,
        };
    }

    pub fn set_disp_dimensions(mut self, dimensions: DisplayDimensions) -> Self {
        self.disp_dimensions = dimensions;
        return self;
    }

    pub fn set_text_style(mut self, text_style: MonoTextStyle<'a, BinaryColor>) -> Self {
        self.text_style = text_style;
        return self;
    }

    pub fn set_primitives_style(mut self, primitives_style: PrimitiveStyle<BinaryColor>) -> Self {
        self.primitives_style = primitives_style;
        return self;
    }

    pub fn set_primitives_alternate_style(mut self, primitives_alternate_style: PrimitiveStyle<BinaryColor>) -> Self {
        self.primitives_alternate_style = primitives_alternate_style;
        return self;
    }
}

// ------------------------------------------------------------------------------------------------------------------------------------------------

#[allow(dead_code)]
pub struct CustomTextbox<'a, DI, SIZE>
where
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    text: String<TEXT_BUFFER_SIZE>,

    disp_dimensions: DisplayDimensions,
    display_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,

    text_style: MonoTextStyle<'a, BinaryColor>,
    primitives_style: PrimitiveStyle<BinaryColor>,
    primitives_alternate_style: PrimitiveStyle<BinaryColor>,

    debug: bool,
}

#[allow(dead_code)]
impl<'a, DI, SIZE> CustomTextbox<'a, DI, SIZE>
where
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    pub fn draw(&self, flush: bool) -> Result<(), CustomError> {

        let mut display_refmut = self.display_refcell.borrow_mut();
        let display_ref = display_refmut.deref_mut();

        let text_height = self.text_style.font.character_size.height as u8 - PIXELS_REMOVED;
        let textbox_height = text_height + TEXTBOX_OFFSET; // The height of the whole textbox is the height of one line of text plus the offset

        Rectangle::with_corners(
            (0, self.disp_dimensions.height as i32 - 1).into(), // Bottom right corner
            (
                self.disp_dimensions.width as i32 - 1,
                (self.disp_dimensions.height - textbox_height) as i32
            ).into() // Top left corner
        )
        .into_styled(
            if self.debug {
                self.primitives_style // If we're in debug mode, we use the normal style to draw white boundaries
            } else {
                self.primitives_alternate_style // Otherwise, we use the alternate style to draw a black rectangle - clearing the area
            }
        )
        .draw(display_ref)?;

        Text::with_baseline(
            self.text.as_str(),
            (0, (self.disp_dimensions.height - textbox_height) as i32).into(), // Top left corner
            self.text_style,
            Baseline::Top
        )
        .draw(display_ref)?;

        if TEXTBOX_CURSOR {
            let cursor_height = TEXTBOX_OFFSET - 1;

            // Draw the cursor under the text
            Rectangle::new(
                (
                    self.text.chars().count() as i32 * self.text_style.font.character_size.width as i32 + 1, 
                    (self.disp_dimensions.height - 1 - cursor_height) as i32
                ).into(),
                (self.text_style.font.character_size.width, cursor_height as u32).into()
            )
            .into_styled(self.primitives_style)
            .draw(display_ref)?;
        }

        if flush { display_ref.flush()?; };

        Ok(())
    }

    pub fn append_str(&mut self, string: &str) -> Result<(), CustomError> {
        // We do not check for buffer overflow, as `push_str` will do that for us
        // `heapless` v0.9 changed the error type of `push` and `push_str` from `()` to `CapacityError`
        if let Err(e) = self.text.push_str(string) {
            warn!("Tried to append a string that is too long for the textbox, returning Err.");
            return Err(CustomError::from(e));
        };
        Ok(())
    }

    pub fn append_char(&mut self, c: char) -> Result<(), CustomError> {
        if let Err(e) = self.text.push(c) {
            warn!("Tried to append a character that is too long for the textbox, returning Err.");
            return Err(CustomError::from(e));
        };
        Ok(())
    }

    pub fn get_text(&self) -> String<TEXT_BUFFER_SIZE> {
        return self.text.clone();
    }

    pub fn backspace(&mut self, count: usize) -> Result<(), CustomError> {
        if self.text.len() < count {
            warn!("Tried to backspace more than is present, returning an Err.");
            return Err(CustomError::BadInput);
        }

        for _ in 0..count {
            self.text.pop().expect("We already checked, this shouldn't be possible!");
        }
        return Ok(());
    }

    // core::str::pattern::Pattern like in str::contains is unstable, so we only implement the char version for now
    // For more info, see: https://github.com/rust-lang/rust/issues/27721
    pub fn contains(&self, pat: char) -> bool {
        return self.text.contains(pat);
    }

    pub fn clear(&mut self) {
        //warn!("Clearing the textbox, all text will be lost.");
        self.text.clear();
    }

    pub fn len(&self) -> usize {
        return self.text.len();
    }

    pub fn is_empty(&self) -> bool {
        self.text.len() == 0
    }
}