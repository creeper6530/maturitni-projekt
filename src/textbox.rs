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

use crate::custom_error::CustomError; // Because we already have the `mod` in `main.rs`
use CustomError as CE; // Shorter alias

// ------------------------------------------------------------------------------------------------------------------------------------------------

// Note: these constants are copied in `stack.rs` as well, maintain consistency between the two files!

// Compile time constants
/** The fonts we use usually have unused pixels at the top that'd waste space,
so with this constant we basically cut off the top `n` pixels. */
const PIXELS_REMOVED: u8 = 2;
/// Size of String-s used for buffering text during writes, and for the textbox
const TEXT_BUFFER_SIZE: usize = 32;
/// Message to fill the textbox with when building a debug textbox
const DEBUG_TEXTBOX_MESSAGE: &'static str = "DEBUG TEXTBOX";
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
    if TEXT_BUFFER_SIZE < DEBUG_TEXTBOX_MESSAGE.len() {
        core::panic!("TEXT_BUFFER_SIZE is too small to hold DEBUG_TEXTBOX_MESSAGE");
    }
}
const _: () = _check_consts(); // Trigger the const fn to check the constants at compile time

// ------------------------------------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayDimensions {
    pub width: u8,
    pub height: u8,
}

impl From<(u8, u8)> for DisplayDimensions {
    fn from(dimensions: (u8, u8)) -> Self {
        DisplayDimensions {
            width: dimensions.0,
            height: dimensions.1,
        }
    }
}

impl Default for DisplayDimensions {
    fn default() -> Self {
        DisplayDimensions {
            width: 128,
            height: 64,
        }
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

    character_style: MonoTextStyle<'a, BinaryColor>,
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
        CustomTextboxBuilder {
            text: String::new(),
            
            disp_dimensions: DisplayDimensions::default(),
            display_refcell,

            // Standard white text on (by default) transparent background
            character_style: MonoTextStyle::new(&ISO_FONT_6X12, BinaryColor::On),

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
        }
    }

    pub fn build(self) -> CustomTextbox<'a, DI, SIZE> {
        CustomTextbox {
            text: self.text,

            disp_dimensions: self.disp_dimensions,
            display_refcell: self.display_refcell,

            character_style: self.character_style,
            primitives_style: self.primitives_style,
            primitives_alternate_style: self.primitives_alternate_style,
        }
    }

    pub fn build_debug(mut self) -> CustomTextbox<'a, DI, SIZE> {
        // The String should be empty at this point
        debug_assert!(self.text.is_empty(), "Tried to build a debug textbox, but the textbox text is not empty!");
        
        self.text.push_str(DEBUG_TEXTBOX_MESSAGE)
            .expect("TEXT_BUFFER_SIZE is too small for DEBUG_TEXTBOX_MESSAGE, this should be impossible!"); // We checked at compile time

        CustomTextbox {
            text: self.text,

            disp_dimensions: self.disp_dimensions,
            display_refcell: self.display_refcell,

            character_style: self.character_style,
            primitives_style: self.primitives_style,
            primitives_alternate_style: self.primitives_alternate_style,
        }
    }

    pub fn set_disp_dimensions(mut self, dimensions: DisplayDimensions) -> Self {
        self.disp_dimensions = dimensions;
        self
    }

    pub fn set_character_style(mut self, character_style: MonoTextStyle<'a, BinaryColor>) -> Self {
        self.character_style = character_style;
        self
    }

    pub fn set_primitives_style(mut self, primitives_style: PrimitiveStyle<BinaryColor>) -> Self {
        self.primitives_style = primitives_style;
        self
    }

    pub fn set_primitives_alternate_style(mut self, primitives_alternate_style: PrimitiveStyle<BinaryColor>) -> Self {
        self.primitives_alternate_style = primitives_alternate_style;
        self
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

    character_style: MonoTextStyle<'a, BinaryColor>,
    primitives_style: PrimitiveStyle<BinaryColor>,
    primitives_alternate_style: PrimitiveStyle<BinaryColor>,
}

/// Can return DisplayError only
#[allow(dead_code)]
impl<'a, DI, SIZE> CustomTextbox<'a, DI, SIZE>
where
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    pub fn draw(&self, flush: bool) -> Result<(), CustomError> {

        let mut display_refmut = self.display_refcell.borrow_mut();
        let display_ref = display_refmut.deref_mut();

        let text_height = self.character_style.font.character_size.height as u8 - PIXELS_REMOVED;
        let textbox_height = text_height + TEXTBOX_OFFSET; // The height of the whole textbox is the height of one line of text plus the offset

        Rectangle::with_corners(
            (0, self.disp_dimensions.height as i32 - 1).into(), // Bottom right corner
            (
                self.disp_dimensions.width as i32 - 1,
                (self.disp_dimensions.height - textbox_height) as i32
            ).into() // Top left corner
        )
        .into_styled(self.primitives_alternate_style)
        .draw(display_ref)?;

        Text::with_baseline(
            self.text.as_str(),
            (0, (self.disp_dimensions.height - textbox_height) as i32).into(), // Top left corner
            self.character_style,
            Baseline::Top
        )
        .draw(display_ref)?;

        if TEXTBOX_CURSOR {
            let cursor_height = TEXTBOX_OFFSET - 1;

            // Draw the cursor under the text
            Rectangle::new(
                (
                    self.text.chars().count() as i32 * self.character_style.font.character_size.width as i32 + 1, 
                    (self.disp_dimensions.height - 1 - cursor_height) as i32
                ).into(),
                (self.character_style.font.character_size.width, cursor_height as u32).into()
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

        // We don't need e.into() for the zero-sized CapacityError,
        // and like this it's clearer than Ok(push_str(...)?)
        self.text.push_str(string).map_err(|_| CE::CapacityError)
    }

    pub fn append_char(&mut self, c: char) -> Result<(), CustomError> {
        self.text.push(c).map_err(|_| CE::CapacityError)
    }

    /// Returns a cloned String of the textbox's text
    pub fn get_text(&self) -> String<TEXT_BUFFER_SIZE> {
        self.text.clone()
    }
    /// Returns a string slice of the textbox's text – only a reference, no cloning
    pub fn get_text_str(&self) -> &str {
        self.text.as_str()
    }

    pub fn backspace(&mut self, count: usize) -> Result<(), CustomError> {
        if self.text.len() < count {
            return Err(CE::BadInput);
        }

        if self.text.is_ascii() {
            // More efficient, but in current implementation requires ASCII-only text
            // In my unscientific benchmarks, this is ~85 µs faster for 1 character on dev build
            // Grace Hopper would be proud, that's a save of about 85 000 nanoseconds! :D
            self.text.truncate(self.text.len() - count);
        } else {
            // Fallback, could be slower
            for _ in 0..count {
                self.text.pop().expect("We already checked, this shouldn't be possible!");
            }
        }
        
        Ok(())
    }

    // core::str::pattern::Pattern trait like in str::contains is unstable, so we implement the char and str versions separately.
    // Too much hassle to implement a custom trait or to use nightly just for this.
    // For more info, see: https://github.com/rust-lang/rust/issues/27721
    pub fn contains(&self, pat: char) -> bool {
        self.text.contains(pat)
    }
    pub fn contains_str(&self, pat: &str) -> bool {
        self.text.contains(pat)
    }

    pub fn starts_with(&self, pat: char) -> bool {
        self.text.starts_with(pat)
    }
    pub fn starts_with_str(&self, pat: &str) -> bool {
        self.text.starts_with(pat)
    }

    pub fn ends_with(&self, pat: char) -> bool {
        self.text.ends_with(pat)
    }
    pub fn ends_with_str(&self, pat: &str) -> bool {
        self.text.ends_with(pat)
    }

    pub fn insert_at(&mut self, index: usize, c: char) -> Result<(), CustomError> {
        if index > self.text.len() {
            return Err(CE::BadInput);
        }
        if !self.text.is_char_boundary(index) {
            return Err(CE::BadInput);
        }
        
        // Checks for capacity overflow by itself
        self.text.insert(index, c)?;
        Ok(())
    }
    pub fn insert_str_at(&mut self, index: usize, string: &str) -> Result<(), CustomError> {
        if index > self.text.len() {
            return Err(CE::BadInput);
        }
        if !self.text.is_char_boundary(index) {
            return Err(CE::BadInput);
        }
        
        // Checks for capacity overflow by itself
        self.text.insert_str(index, string)?;
        Ok(())
    }

    pub fn remove_at(&mut self, index: usize) -> Result<char, CustomError> {
        if index >= self.text.len() {
            return Err(CE::BadInput);
        }
        if !self.text.is_char_boundary(index) {
            return Err(CE::BadInput);
        }

        Ok(self.text.remove(index))
    }

    pub fn clear(&mut self) {
        //warn!("Clearing the textbox, all text will be lost.");
        self.text.clear();
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.len() == 0
    }
}