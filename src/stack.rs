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

// Compile time constants
/// Maximum size of the stack
const MAX_STACK_SIZE: usize = 256;
/// Maximum number of elements to pop at once
const MAX_MULTIPOP_SIZE: usize = 16;

// ------------------------------------------------------------------------------------------------------------------------------------------------

/// All getters of this struct copy the data, not give a reference to it.
#[allow(dead_code)]
pub struct CustomStack<T>
where
    T: Copy + core::fmt::Debug, /* The `T` type parameter allows the stack to hold any type of data
                that implements the `Copy` trait (which means they can be duplicated easily),
                and that can be formatted. */
{
    data: Vec<T, MAX_STACK_SIZE>
}

#[allow(dead_code)]
impl<T> CustomStack<T>
where
    T: Copy + core::fmt::Debug,
{
    /// Creates a new, empty stack.
    pub fn new() -> Self {
        CustomStack {
            data: Vec::new(),
        }
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