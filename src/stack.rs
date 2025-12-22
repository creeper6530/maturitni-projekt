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

// Possibly gate this behind a defmt feature flag if we move this into a library crate
use defmt::trace; // For logging in `draw()` (nowhere else)

use crate::custom_error::CustomError; // Because we already have the `mod` in `main.rs`
use CustomError as CE; // Shorter alias

// ------------------------------------------------------------------------------------------------------------------------------------------------

// Note: these constants are copied in `textbox.rs` as well, maintain consistency between the two files!

// Compile time constants
/// Maximum size of the stack
const MAX_STACK_SIZE: usize = 256;
/** The fonts we use usually have unused pixels at the top that'd waste space,
so with this constant we basically cut off the top `n` pixels. */
const PIXELS_REMOVED: u8 = 2;
/// Size of String-s used for buffering text during writes, and for the textbox
const TEXT_BUFFER_SIZE: usize = 32;
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
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    data: Vec<T, MAX_STACK_SIZE>,

    display_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,
    disp_dimensions: DisplayDimensions,

    character_style: MonoTextStyle<'a, BinaryColor>,
    primitives_style: PrimitiveStyle<BinaryColor>,
}

#[allow(dead_code)]
impl<'a, T, DI, SIZE> CustomStackBuilder<'a, T, DI, SIZE>
where
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

            // Standard black stroke and fill (i.e. all black, effectively erasing anything drawn below it)
            primitives_style: PrimitiveStyleBuilder::new()
                .stroke_color(BinaryColor::Off)
                .fill_color(BinaryColor::Off)
                .build(),
        }
    }

    pub fn build(self) -> CustomStack<'a, T, DI, SIZE> {
        defmt::assert_eq!(self.data.capacity(), MAX_STACK_SIZE); // Just to be sure

        CustomStack {
            data: self.data,

            disp_dimensions: self.disp_dimensions,
            display_refcell: self.display_refcell,

            character_style: self.character_style,
            primitives_style: self.primitives_style,
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
}

#[allow(dead_code)]
impl<'a, T, DI, SIZE> CustomStackBuilder<'a, T, DI, SIZE>
where
    T: Default + Clone, // It is exceptionally rare that a type is Default but not Clone, so this is acceptable.
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    pub fn build_debug(mut self) -> CustomStack<'a, T, DI, SIZE> {
        self.data.resize_default(MAX_STACK_SIZE)
            // This Result does not have T as its Err type, so no Debug bound arises here
            .expect("We're resizing to MAX_STACK_SIZE, so this should never fail!");

        CustomStack {
            data: self.data,

            disp_dimensions: self.disp_dimensions,
            display_refcell: self.display_refcell,

            character_style: self.character_style,
            primitives_style: self.primitives_style,
        }
    }
}

// ------------------------------------------------------------------------------------------------------------------------------------------------

/// All getters of this struct copy the data, not give a reference to it.
#[allow(dead_code)]
pub struct CustomStack<'a, T, DI, SIZE>
where // We dropped the Copy bound in commit b570971032f7a7de6d69c37402bddd0ee0cb40b2 and Debug/Display in commit right after
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    data: Vec<T, MAX_STACK_SIZE>,

    disp_dimensions: DisplayDimensions,
    display_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,

    character_style: MonoTextStyle<'a, BinaryColor>,
    primitives_style: PrimitiveStyle<BinaryColor>,
}

#[allow(dead_code)]
impl<'a, T, DI, SIZE> CustomStack<'a, T, DI, SIZE>
where
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    /// Pushes a value onto the stack.
    /// 
    /// We need ownership of the value to push it onto the stack.
    /// (In reality it's trivial to since DecimalFixed we use is Copy)
    /// 
    /// In Err we return a tuple including the value that was attempted to be pushed,
    /// so that the caller can decide what to do with it.
    pub fn push(&mut self, value: T) -> Result<(), (CustomError, T)> {
        self.data.push(value).map_err(|t| (CE::CapacityError, t))
    }

    /// Pushes multiple values onto the stack from any iterator.
    /// If the stack does not have enough space for all values, it panics.
    /// 
    /// For slices that are composed of Clone types, prefer `push_slice()`.
    /// For exact-size iterators, prefer `push_exact_iterator()` that can't panic.
    /// 
    /// If `check_hint` is true, and the iterator has an upper size hint,
    /// and the hint indicates that pushing all elements would overflow the stack, 
    /// the method checks if the upper size hint would overflow the stack
    /// and returns an error instead of possibly panicking.
    /// 
    /// E.g. for a Vec, setting `check_hint` to true is recommended, because the size hint is exact.
    pub fn push_iterator(&mut self, iter: impl Iterator<Item = T>, check_hint: bool) -> Result<(), CustomError> {
        // Short-circuiting is desirable here
        if check_hint // If the caller wants us to check the size hint
            && let Some(hint) = iter.size_hint().1 // If the iterator has an upper bound
            && self.data.len() + hint > MAX_STACK_SIZE // If it would actually overflow the stack
        {
            return Err(CE::CapacityError);
        }

        self.data.extend(iter);
        Ok(())
    }

    /// Pushes multiple values onto the stack from an exact-size iterator.
    /// If the stack does not have enough space for all values, it returns an error.
    /// 
    /// For slices that are composed of Clone types, prefer `push_slice()`.
    /// For general iterators, use `push_iterator()` if the possibility of panic does not scare you.
    pub fn push_exact_iterator(&mut self, iter: impl ExactSizeIterator<Item = T>) -> Result<(), CustomError> {
        let iter = iter.into_iter();
        
        // Thanks to ExactSizeIterator, we can get the exact size of the iterator beforehand and check for overflow
        if self.data.len() + iter.len() > MAX_STACK_SIZE {
            return Err(CE::CapacityError);
        }

        self.data.extend(iter);
        Ok(())
    }

    /// Pops a value from the stack.
    /// If the stack is empty, it returns `None`.
    pub fn pop(&mut self) -> Option<T> {
        self.data.pop()
    }

    /// Returns a double-ended iterator that yields up to `n` popped elements from the stack.
    /// Each `.next()` call on the iterator pops one, but the rest are still popped even if the iterator is dropped before being fully consumed.
    /// 
    /// If the stack is empty, it returns `None`. If you try to pop more elements than there are, it pops all.
    /// 
    /// Note that the returned iterator still keeps a mutable borrow on the stack until it is fully consumed or dropped.
    /// 
    /// The last item yielded by the iterator is the topmost element of the stack.
    pub fn multipop(&mut self, n: u8) -> Option<impl DoubleEndedIterator<Item = T>> {
    // See https://doc.rust-lang.org/stable/book/ch10-02-traits.html#returning-types-that-implement-traits for explanation of what we're returning here.
        if self.data.is_empty() {
            return None;
        }

        // The caller may not need to collect it into a Vec, so we return the iterator directly.
        // If the iterator is dropped before it's fully consumed, the data is still removed from the stack.
        // Thanks to saturating_sub, we don't have to check if n > len here.
        Some(self.data.drain(self.data.len().saturating_sub(n as usize)..))
    }

    /// Returns the last value pushed onto the stack without removing it.
    /// If the stack is empty, it returns `None`.
    pub fn peek(&self) -> Option<&T> {
        self.data.last()
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
            return None;
        }

        Some(&self.data[self.data.len().saturating_sub(n)..]) // Get the last `n` elements as a slice.
        // The `saturating_sub` ensures that if n > len, we get 0 as the start index, effectively returning the entire stack.
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
    T: Clone, // We need Clone here to be able to clone the slice elements (since we can't own the slice)
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    /// Pushes multiple values onto the stack from a slice.
    /// If the stack does not have enough space for all values, it returns an error.
    ///
    /// We need the `Clone` bound on `T` to be able to clone the elements from the slice.
    /// To push multiple values from an IntoIterator collection that owns the values, use `push_iterator()`.
    pub fn push_slice(&mut self, slice: &[T]) -> Result<(), CustomError> {
        self.data.extend_from_slice(slice).map_err(|_| CE::CapacityError)
    }
}

impl<'a, T, DI, SIZE> CustomStack<'a, T, DI, SIZE>
where
    T: core::fmt::Display,
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
        .into_styled(self.primitives_style)
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

        // Possibly gate this behind a defmt feature flag if we move this into a library crate
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

            core::write!(&mut buf, "{}", text_vec[i_usize])?;
            let text = buf.as_str();

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
}