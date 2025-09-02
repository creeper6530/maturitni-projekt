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

// ------------------------------------------------------------------------------------------------------------------------------------------------

// Note: these constants are copied in `textbox.rs` as well, maintain consistency between the two files!

// Compile time constants
/// Maximum size of the stack
const MAX_STACK_SIZE: usize = 256;
/// Maximum number of elements to pop at once
const MAX_MULTIPOP_SIZE: usize = 16;
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

pub struct CustomStackBuilder<'a, T, DI, SIZE>
where
    T: Copy + core::fmt::Debug + core::fmt::Display,
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    data: Vec<T, MAX_STACK_SIZE>,

    display_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,
    disp_dimensions: DisplayDimensions,

    text_style: MonoTextStyle<'a, BinaryColor>,
    primitives_style: PrimitiveStyle<BinaryColor>,
    primitives_alternate_style: PrimitiveStyle<BinaryColor>,
}

#[allow(dead_code)]
impl<'a, T, DI, SIZE> CustomStackBuilder<'a, T, DI, SIZE>
where
    T: Copy + core::fmt::Debug + core::fmt::Display,
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
        return CustomStackBuilder::<'a, T, DI, SIZE> {
            data: Vec::new(), // The <T, MAX_STACK_SIZE> is inferred from the type parameters in the struct definition
            
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

    pub fn build(self) -> CustomStack<'a, T, DI, SIZE> {
        return CustomStack {
            data: self.data,

            disp_dimensions: self.disp_dimensions,
            display_refcell: self.display_refcell,

            text_style: self.text_style,
            primitives_style: self.primitives_style,
            primitives_alternate_style: self.primitives_alternate_style,

            debug: false,
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

#[allow(dead_code)]
impl<'a, T, DI, SIZE> CustomStackBuilder<'a, T, DI, SIZE>
where
    T: Copy + core::fmt::Debug + core::fmt::Display + Default, // We add Default here so that we can use `T::default()`
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    pub fn build_debug(mut self) -> CustomStack<'a, T, DI, SIZE> {
        warn!("Building a debug stack, filling it with default values.");

        // Fill the stack with default values
        for i in [T::default(); MAX_STACK_SIZE] {
            self.data.push(i).unwrap();
        }

        return CustomStack {
            data: self.data,

            disp_dimensions: self.disp_dimensions,
            display_refcell: self.display_refcell,

            text_style: self.text_style,
            primitives_style: self.primitives_style,
            primitives_alternate_style: self.primitives_alternate_style,

            debug: true,
        };
    }
}

// ------------------------------------------------------------------------------------------------------------------------------------------------

/// All getters of this struct copy the data, not give a reference to it.
#[allow(dead_code)]
pub struct CustomStack<'a, T, DI, SIZE>
where
    T: Copy + core::fmt::Debug + core::fmt::Display, /* The `T` type parameter allows the stack to hold any type of data
                that implements the `Copy` trait (which means they can be duplicated easily),
                and that can be formatted. */
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    data: Vec<T, MAX_STACK_SIZE>,

    disp_dimensions: DisplayDimensions,
    display_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,

    text_style: MonoTextStyle<'a, BinaryColor>,
    primitives_style: PrimitiveStyle<BinaryColor>,
    primitives_alternate_style: PrimitiveStyle<BinaryColor>,

    debug: bool,
}

#[allow(dead_code)]
impl<'a, T, DI, SIZE> CustomStack<'a, T, DI, SIZE>
where
    T: Copy + core::fmt::Debug + core::fmt::Display,
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    /// # WIP
    /// ## TODO: Get error handling better than panickings
    /// 
    /// Draws the stack on the display.
    pub fn draw(&self, flush: bool) {

        // We're going to operate on the display for the entire method, so no need to wrap it in a scope
        // It will get automatically dropped at the end of the method
        let mut display_refmut = self.display_refcell.borrow_mut();
        let display_ref = display_refmut.deref_mut(); // Get a mutable reference to the display itself, no RefMut

        // A convenience variable
        let text_height = (self.text_style.font.character_size.height - PIXELS_REMOVED as u32) as u8;

        // If there is less data than the display can show, we just draw all of it.
        // In that case, we will "hang" the stack visually from the top of the display (desirable).
        let num_lines = min(
            self.data.len() as u8,
            (self.disp_dimensions.height / text_height // Integer division: always rounded down (desirable here)
            ) - 1 // -1 because we want to leave space for the bottom line
        );
        trace!("Drawing {} lines on the display.", num_lines);
        
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
        .draw(display_ref)
        .unwrap();

        if self.data.is_empty() {
            // If the stack is empty, we don't need to draw anything so we expediently return
            if flush { display_ref.flush().unwrap(); };
            return;
        }

        let text_vec = self.multipeek(num_lines)
            .unwrap_or(Vec::new()); // If the stack is empty, we get an empty Vec

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
                core::write!(&mut buf, "{:?}", text_vec[i_usize]).unwrap();
                buf.as_str()
            } else {
                // Otherwise, we just print the value as is
                core::write!(&mut buf, "{}", text_vec[i_usize]).unwrap();
                buf.as_str()
            };

            Text::with_baseline(
                text,
                (0, ((self.text_style.font.character_size.height as u8 - PIXELS_REMOVED) * i) as i32).into(),
                self.text_style,
                Baseline::Top
            )
            .draw(display_ref)
            .unwrap();
        }

        if flush { display_ref.flush().unwrap(); };
    }

    /// Pushes a value onto the stack.
    /// If the stack is full, it returns an error with the value that could not be pushed.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        let pushed = self.data.push(value);
        if pushed.is_err() {
            warn!("Tried to push a value onto a full stack, returning Err.");
        }
        return pushed;
    }

    pub fn push_slice(&mut self, slice: &[T]) -> Result<(), ()> {
        if self.data.len() + slice.len() > MAX_STACK_SIZE {
            warn!("Tried to push a slice onto the stack that would overflow it, returning Err.");
            return Err(());
        }

        return self.data.extend_from_slice(slice).map_err(|_| ()); // Technically could be unwrapped because of the check above, but better safe than sorry
    }

    /// Pops a value from the stack.
    /// If the stack is empty, it returns `None`.
    pub fn pop(&mut self) -> Option<T> {
        let popped = self.data.pop();

        match popped {
            None => {
                warn!("Tried to pop from an empty stack, returning None.");
                return None
            },
            Some(value) => return Some(value) // We don't need to clone here, since `T` is `Copy`
        }
    }

    /// Pops `n` elements from the stack and returns them as a slice.
    /// If `n` is greater than the stack size, it returns the entire stack as a slice.
    /// If the stack is empty, it returns `None`.
    /// 
    /// The topmost element is the last element in the returned vector.
    /// 
    /// A const controls the maximum number of elements that can be popped at once.
    /// We need the Vec because returning a slice would not copy the data, just reference them.
    pub fn multipop(&mut self, n: u8) -> Option<Vec<T, MAX_MULTIPOP_SIZE>> {
        let peeked = self.multipeek(n)?; // Use `multipeek` to get the last `n` elements with all the checks
        // Notice that we use `?` to handle the case where `multipeek` returns `None`

        for _ in 0..(peeked.len()) {
            self.data.pop(); // Pop the elements from the stack, drop the output, since we already have them in `peeked`
        }

        return Some(peeked);
    }

    /// Returns the last value pushed onto the stack without removing it.
    /// If the stack is empty, it returns `None`.
    pub fn peek(&self) -> Option<T> {
        let last = self.data.last();

        match last {
            None => {
                warn!("Tried to peek into an empty stack, returning None.");
                return None;
            },
            Some(value) => return Some(*value) // We can dereference the last element instead of .clone() since `T` is `Copy`
        }
    }

    /// Returns the last `n` values pushed onto the stack without removing them as a slice.
    /// If `n` is greater than the stack size, it returns the entire stack as a slice.
    /// If the stack is empty, it returns `None`.
    /// 
    /// The topmost element is the last element in the returned slice.
    /// 
    /// A const controls the maximum number of elements that can be popped at once.
    /// We need the Vec because returning a slice would not copy the data, just reference them.
    pub fn multipeek(&self, n: u8) -> Option<Vec<T, MAX_MULTIPOP_SIZE>> {
        let n = n as usize;

        if self.data.is_empty() {
            warn!("Tried to peek into an empty stack, returning None.");
            return None;
        }
        
        if n > self.data.len() {
            warn!("Tried to peek further than the stack size, returning the entire stack.");
            return Vec::from_slice(self.data.as_slice()).ok(); // Not very obvious, but we are cloning the slice
        }

        let slice = &self.data[self.data.len() - n..]; // Get the last `n` elements as a slice

        return Vec::from_slice(slice).ok();
    }

    /// Clears the entire stack
    /// 
    /// ## Warning
    /// This method will cause all data in the stack to be lost.
    pub fn clear(&mut self) {
        warn!("Clearing the stack, all data will be lost.");
        self.data.clear();
    }

    pub fn len(&self) -> usize {
        return self.data.len();
    }
}