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

// Note: these constants are copied in `textbox.rs` as well, maintain consistency between the two files!

// Compile time constants
/// Maximum size of the stack
const MAX_STACK_SIZE: usize = 256;
/// Maximum number of elements to pop at once
const MAX_MULTIPOP_SIZE: usize = 16;
/// Maximum number of elements to push at once
const MAX_VEC_PUSH_SIZE: usize = 16;
/** The fonts we use usually have unused pixels at the top that'd waste space,
so with this constant we basically cut off the top `n` pixels. */
const PIXELS_REMOVED: u8 = 2;
/// Size of String-s used for buffering text during writes, and for the textbox
const TEXT_BUFFER_SIZE: usize = 32;

// Evaluated at compile time to ensure that the constants are valid
const fn _check_consts() {
    if MAX_MULTIPOP_SIZE > MAX_STACK_SIZE {
        core::panic!("MAX_MULTIPOP_SIZE must not be greater than MAX_STACK_SIZE");
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

pub struct CustomStackBuilder<'a, T, DI, SIZE>
where
    T: core::fmt::Debug + core::fmt::Display,
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    data: Vec<T, MAX_STACK_SIZE>,

    display_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,
    disp_dimensions: DisplayDimensions,

    character_style: MonoTextStyle<'a, BinaryColor>,
    primitives_style: PrimitiveStyle<BinaryColor>,
    primitives_alternate_style: PrimitiveStyle<BinaryColor>,
}

#[allow(dead_code)]
impl<'a, T, DI, SIZE> CustomStackBuilder<'a, T, DI, SIZE>
where
    T: core::fmt::Debug + core::fmt::Display,
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    /// Creates a new `CustomStack` with the given display RefCell.
    /// 
    /// This constructor uses the default display dimensions of 128x64 pixels and the default text style.
    /// For custom parameters, use [`Self::new_custom()`].
    pub fn new(
        display_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>
    ) -> Self {
        CustomStackBuilder::<'a, T, DI, SIZE> {
            data: Vec::new(), // The <T, MAX_STACK_SIZE> is inferred from the type parameters in the struct definition
            
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

    pub fn build(self) -> CustomStack<'a, T, DI, SIZE> {
        CustomStack {
            data: self.data,

            disp_dimensions: self.disp_dimensions,
            display_refcell: self.display_refcell,

            character_style: self.character_style,
            primitives_style: self.primitives_style,
            primitives_alternate_style: self.primitives_alternate_style,

            debug: false,
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

#[allow(dead_code)]
impl<'a, T, DI, SIZE> CustomStackBuilder<'a, T, DI, SIZE>
where
    T: core::fmt::Debug + core::fmt::Display + Default, // We add Default here so that we can use `T::default()`
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    pub fn build_debug(mut self) -> CustomStack<'a, T, DI, SIZE> {
        warn!("Building a debug stack, filling it with default values.");

        // Fill the stack with default values
        for _ in 0..MAX_STACK_SIZE { // Do it MAX_STACK_SIZE times
            self.data.push(T::default()).expect("We're pushing exactly MAX_STACK_SIZE elements, so this should never fail!");
        }

        CustomStack {
            data: self.data,

            disp_dimensions: self.disp_dimensions,
            display_refcell: self.display_refcell,

            character_style: self.character_style,
            primitives_style: self.primitives_style,
            primitives_alternate_style: self.primitives_alternate_style,

            debug: true,
        }
    }
}

// ------------------------------------------------------------------------------------------------------------------------------------------------

/// All getters of this struct copy the data, not give a reference to it.
#[allow(dead_code)]
pub struct CustomStack<'a, T, DI, SIZE>
where
    T: core::fmt::Debug + core::fmt::Display, // We dropped the Copy bound in commit after commit 49de9b4250602eb917f56fd749881d5173e65bbb
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    data: Vec<T, MAX_STACK_SIZE>,

    disp_dimensions: DisplayDimensions,
    display_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,

    character_style: MonoTextStyle<'a, BinaryColor>,
    primitives_style: PrimitiveStyle<BinaryColor>,
    primitives_alternate_style: PrimitiveStyle<BinaryColor>,

    debug: bool,
}

#[allow(dead_code)]
impl<'a, T, DI, SIZE> CustomStack<'a, T, DI, SIZE>
where
    T: core::fmt::Debug + core::fmt::Display,
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    /// Draws the stack on the display.
    /// Can return DisplayError or FormatError.
    pub fn draw(&self, flush: bool) -> Result<(), CustomError> {

        // We're going to operate on the display for the entire method, so no need to wrap it in a scope
        // It will get automatically dropped at the end of the method
        let mut display_refmut = self.display_refcell.borrow_mut();
        let display_ref = display_refmut.deref_mut(); // Get a mutable reference to the display itself, no RefMut

        // A convenience variable
        let text_height = (self.character_style.font.character_size.height - PIXELS_REMOVED as u32) as u8;
        
        // Clear the area where the stack will be drawn
        Rectangle::new(
            (0, 0).into(),
            (self.disp_dimensions.width as u32, (text_height * ((self.disp_dimensions.height / text_height) - 1)) as u32).into() // We always clear the entire area, e.g. when popping elements
        )
        .into_styled(
            if self.debug {
                self.primitives_style // If we're in debug mode, we use the normal style to draw white boundaries
            } else {
                self.primitives_alternate_style // Otherwise, we use the alternate style to draw a black rectangle - clearing the area
            }
        )
        .draw(display_ref)?;

        if self.data.is_empty() {
            // If the stack is empty, we don't need to draw anything so we expediently return
            if flush { display_ref.flush()?; };
            return Ok(());
        }

        // If there is less data than the display can show, we just draw all of it.
        // In that case, we will "hang" the stack visually from the top of the display (desirable).
        let num_lines = min(
            self.data.len() as u8,
            (self.disp_dimensions.height / text_height // Integer division: always rounded down (desirable here)
            ) - 1 // -1 because we want to leave space for the bottom line
        );
        trace!("Drawing {} lines on the display.", num_lines);

        let text_vec = self.multipeek(num_lines).expect("We just checked the Vec is empty!");

        /* We do an engineer's estimate that 32 bytes is enough for one line,
        since we can't compute it dynamically from font size.
        It's true that we don't wanna waste memory, but better safe than sorry.
        At the smallest inbuilt font size, we can fit exactly 32 characters in a line,
        so that's why we use 32 here.

        If we had used i128-s (and didn't do fixed-point arithmetics with them),
        we'd've needed at most 40 bytes (the lenght of i128::MIN in decimal representation),
        but that'd long overflow the display, so who cares? :D */
        let mut buf = String::<TEXT_BUFFER_SIZE>::new();

        for i in (0..num_lines).rev() {
            let i_usize = i as usize; // Convert to usize for indexing
            buf.clear();

            let text: &str = if self.debug {
                // If we're in debug mode, we print the value of the element
                core::write!(&mut buf, "{:?}", text_vec[i_usize])?;
                buf.as_str()
            } else {
                // Otherwise, we just print the value as is
                core::write!(&mut buf, "{}", text_vec[i_usize])?;
                buf.as_str()
            };

            Text::with_baseline(
                text,
                (0, ((self.character_style.font.character_size.height as u8 - PIXELS_REMOVED) * i) as i32).into(),
                self.character_style,
                Baseline::Top
            )
            .draw(display_ref)?;
        }

        if flush { display_ref.flush()?; };
        Ok(())
    }

    /// Pushes a value onto the stack.
    /// If the stack is full, it returns an error with the value that could not be pushed.
    /// 
    /// We need ownership of the value to push it onto the stack.
    /// (In reality it's trivial to since DecimalFixed we use is Copy)
    pub fn push(&mut self, value: T) -> Result<(), CustomError> {
        if self.data.push(value).is_err() {
            warn!("Tried to push a value onto a full stack, returning Err.");
            return Err(CustomError::CapacityError);
        }
        Ok(())
    }

    /// Pushes multiple values onto the stack from a Vec.
    /// If the stack does not have enough space for all values, it returns an error.
    /// 
    /// We use a Vec because it has a known length at runtime and ownership of the data.
    /// For slices that are composed of Clone types, use `push_slice()`.
    pub fn push_vec(&mut self, value: Vec<T, MAX_VEC_PUSH_SIZE>) -> Result<(), CustomError> {
        if self.data.len() + value.len() > MAX_STACK_SIZE {
            warn!("Tried to push a Vec onto the stack that would overflow it, returning Err.");
            return Err(CustomError::CapacityError);
        }
        
        for v in value.into_iter() {
            if self.data.push(v).is_err() {
                error!("We already checked for capacity, so this should never happen!");
                return Err(CustomError::Impossible);
            }
        }
        Ok(())
    }

    /// Pops a value from the stack.
    /// If the stack is empty, it returns `None`.
    pub fn pop(&mut self) -> Option<T> {
        let popped = self.data.pop();

        if popped.is_none() {
            warn!("Tried to pop from an empty stack, returning None.");
        }
        popped
    }

    /// Pops `n` elements from the stack and returns them as a slice.
    /// If `n` is greater than the stack size, it returns the entire stack as a slice.
    /// If the stack is empty, it returns `None`.
    /// 
    /// The topmost element is the last element in the returned vector.
    /// 
    /// A const controls the maximum number of elements that can be popped at once.
    /// We need the Vec because we cannot return a slice that references data that we immediately remove from the stack.
    pub fn multipop(&mut self, n: u8) -> Option<Vec<T, MAX_MULTIPOP_SIZE>> {
        if self.data.is_empty() {
            warn!("Tried to multipop from an empty stack, returning None.");
            return None;
        }

        let iterator = self.data.drain(self.data.len().saturating_sub(n as usize)..);
        // The turbofish isn't strictly necessary, it can infer it from the function signature, but for clarity we leave it here.
        Some(iterator.collect::<Vec<T, MAX_MULTIPOP_SIZE>>())
    }

    /// Returns the last value pushed onto the stack without removing it.
    /// If the stack is empty, it returns `None`.
    pub fn peek(&self) -> Option<&T> {
        let last = self.data.last();

        if last.is_none() {
            warn!("Tried to peek into an empty stack, returning None.");
        }

        last
    }

    /// Returns the last `n` values pushed onto the stack without removing them as a slice.
    /// If `n` is greater than the stack size, it returns the entire stack as a slice.
    /// If the stack is empty, it returns `None`.
    /// 
    /// The topmost element is the last element in the returned slice.
    /// 
    /// For efficiency's sake, we return a slice.
    pub fn multipeek(&self, n: u8) -> Option<&[T]> {
        let n = n as usize; // Shadow the n parameter as usize for easier usage.
        // Perhaps simply changing the parameter type to usize would be better??? Not like it saves any memory, it likely gets passed in a register anyway.
        // TODO: Test it.

        if self.data.is_empty() {
            warn!("Tried to peek into an empty stack, returning None.");
            return None;
        }
        
        if n > self.data.len() {
            warn!("Tried to peek further than the stack size, returning the entire stack.");
            return Some(self.data.as_slice());
        }

        Some(&self.data[self.data.len().saturating_sub(n)..]) // Get the last `n` elements as a slice.
        // The `saturating_sub` is just for safety, does the same as the earlier check, returning the entire stack if n > len, since it's usize.
    }

    /// Clears the entire stack
    /// 
    /// ## Warning
    /// This method will cause all data in the stack to be lost.
    pub fn clear(&mut self) {
        //warn!("Clearing the stack, all data will be lost.");
        self.data.clear();
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

#[allow(dead_code)]
impl<'a, T, DI, SIZE> CustomStack<'a, T, DI, SIZE>
where
    T: core::fmt::Debug + core::fmt::Display + Copy, // We need Copy here to be able to copy the slice elements (since we can't really own the slice)
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    /// Pushes multiple values onto the stack from a slice.
    /// If the stack does not have enough space for all values, it returns an error.
    ///
    /// We need the `Copy` bound on `T` to be able to copy the elements from the slice.
    /// To push multiple values from a Vec that owns the values, use `push_vec()`.
    pub fn push_slice(&mut self, slice: &[T]) -> Result<(), CustomError> {
        if self.data.extend_from_slice(slice).is_err() {
            warn!("Tried to push a slice onto the stack that would overflow it, returning Err.");
            return Err(CustomError::CapacityError);
        };
        Ok(())
    }
}