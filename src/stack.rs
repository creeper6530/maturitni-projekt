use heapless::Vec;
use defmt::Format as DefmtFormat;

// ------------------------------------------------------------------------------------------------------------------------------------------------

// Compile time constants
/// Maximum size of the stack
const MAX_STACK_SIZE: usize = 256;

// ------------------------------------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CustomStack<T>
{
    data: Vec<T, MAX_STACK_SIZE>
}

#[allow(dead_code)]
impl<T> CustomStack<T>
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
        self.data.push(value)
    }

    /// Pushes an owned array of values onto the stack.
    /// The array MUST have a const size, otherwise use `push_slice` or push elements one by one.
    /// If the stack does not have enough space, it returns an error with the array that could not be pushed.
    /// 
    /// The last element of the array will be the topmost element of the stack.
    pub fn push_array<const N: usize>(&mut self, array: [T; N]) -> Result<(), [T; N]> {
        if self.data.len() + N > MAX_STACK_SIZE {
            return Err(array);
        }

        // SAFETY: We already checked that capacity is OK. Can't panic.
        self.data.extend(array);
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
    /// The **FIRST** item yielded by the iterator is the topmost element of the stack.
    pub fn multipop(&mut self, n: u8) -> Option<impl DoubleEndedIterator<Item = T>> {
    // See https://doc.rust-lang.org/stable/book/ch10-02-traits.html#returning-types-that-implement-traits for explanation of what we're returning here.
        if self.data.is_empty() {
            return None;
        }

        // The caller may not need to collect it into a Vec, so we return the iterator directly.
        // If the iterator is dropped before it's fully consumed, the data is still removed from the stack.
        // Thanks to saturating_sub, we don't have to check if n > len here.
        Some(
            self.data.drain(self.data.len().saturating_sub(n as usize)..)
                .rev() // Reverse the iterator so that the first item yielded is the topmost element of the stack.
        )
    }

    /// Optionally returns a reference to the last value
    /// pushed onto the stack without removing it.
    /// Use `.copied()` on the returned `Option<&T>` to get an `Option<T>`.
    /// 
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

    pub fn peek_all(&self) -> &[T] {
        self.data.as_slice()
    }

    /// Clears the entire stack
    /// 
    /// ## Warning
    /// This method will cause all data in the stack to be lost.
    pub fn clear(&mut self) {
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
impl<T> CustomStack<T>
where
    T: Clone,
{
    /// Pushes a slice of values onto the stack.
    /// If the stack does not have enough space, it returns an error.
    /// 
    /// The last element of the slice will be the topmost element of the stack.
    pub fn push_slice(&mut self, slice: &[T]) -> Result<(), ()> {
        if self.data.len() + slice.len() > MAX_STACK_SIZE {
            return Err(());
        }

        self.data.extend_from_slice(slice).expect("We already checked that capacity is OK!");
        Ok(())
    }
}

#[allow(dead_code)]
impl<T> CustomStack<T>
where
    T: DefmtFormat,
{
    /// Debug function to print the entire stack using defmt.
    pub fn debug_print(&self) {
        defmt::debug!("Stack contents: {:?}", self.data.as_slice());
    }
}