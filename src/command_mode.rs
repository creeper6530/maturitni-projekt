use defmt::*;
use rp2040_hal as hal;
use heapless::Vec;
use core::cell::RefCell;

use ssd1306::{prelude::*, Ssd1306, mode::BufferedGraphicsMode};


// Because we already have the `mod` in `main.rs`
use crate::textbox::CustomTextbox;
use crate::stack::CustomStack;
use crate::custom_error::{
    CustomError,
    CE // Short type alias
};

/// # List of commands:
/// 
/// - `reset`: Reset the microcontroller
/// - `halt`: Halt the microcontroller (actually causes a hard fault by executing an undefined instruction)
/// - `breakpoint` (aliases: `bkpt`, `b`): Trigger a breakpoint set in your debugger/IDE
/// - `breakpoint alt` (aliases: `bkpt alt`, `b alt`): Trigger an inline breakpoint instruction (causes exception if no debugger attached)
/// - `boot usb` (aliases: `usb boot`, `usb`): Reboot into the USB bootloader
/// - `redraw` (aliases: `refresh`, `reload`, `r`, `f5`): Force a redraw of stack
///   - Also can be triggered by pressing Ctrl-R, when it also redraws the textbox.
/// - `brightness N` (aliases: `brt N`): Set display brightness to a predefined level between 1 and 5
/// - `clear` (aliases: `cls`, `c`): Clear the stack
/// - `duplicate` (aliases: `dup`): Duplicate the top element of the stack
/// - `drop`: Remove the top element of the stack
///   - `drop N`: Remove the top N elements of the stack (where N is a positive integer not exceeding the current stack size)
/// - `swap` (aliases: `s`): Swap the top two elements of the stack
/// 
/// Empty commands are ignored, pressing Ctrl-C cancels command input.
pub fn handle_commands<'a, DI, SIZE, T, D, P> (
    uart_rx: &'a hal::uart::Reader<D, P>,
    disp_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,
    textbox: &mut CustomTextbox<'a, DI, SIZE>,
    stack: &mut CustomStack<'a, T, DI, SIZE>,
) -> Result<(), CustomError>
where
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
    T: core::fmt::Display, // For the draw() method of the stack
    T: Clone, // Needed for duplicating stack elements

    D: hal::uart::UartDevice,
    P: hal::uart::ValidUartPinout<D>
{
    info!("Entering command mode");
    textbox.clear();
    textbox.draw(true)?;

    { // We limit the scope of the mutable borrow to limit the lifetime of the RefMut and prevent panicking upon double-borrow
        let mut disp = disp_refcell.borrow_mut();
        disp.set_invert(true)?;
    }   

    let mut buf: [u8; 1] = [0];
    let mut char_buf: char; // We declare it uninitialised mutable here to save on repeated stack allocations (as you should with buffers used in a loop)

    // The label is unnecessary, just for clarity
    'read_loop: loop {
        if let Err(e) = uart_rx.read_full_blocking(&mut buf) {
            error!("Failed to read from UART: {:?}", e);
            if let hal::uart::ReadErrorType::Break = e {
                debug!("Check wiring, usually a break indicates a disconnected wire at the RX pin.");
            };
            return Err(e.into());
        };   
        char_buf = char::from_u32(buf[0] as u32).unwrap_or('?'); // Replace invalid UTF-8 with a replacement character

        match char_buf {
            '\x03' => { // Ctrl-C
                info!("Aborting command input on Ctrl-C");
                textbox.clear();
                textbox.draw(true)?;
                {
                    let mut disp = disp_refcell.borrow_mut();
                    disp.set_invert(false)?;
                }
                return Err(CE::Cancelled);
            },
            '\r' | '\n' => break 'read_loop, // Enter key - breaks out of the reading loop
            '\x08' | '\x7F' => { // Backspace
                trace!("Backspace character received in command mode: (0x{:X})", buf[0]);

                if textbox.is_empty() {
                    info!("Ignoring backspace on empty textbox in command mode.");
                    continue 'read_loop; // Diverging, does not continue forwards
                };
                if textbox.backspace(1).is_err() {
                    error!("Failed to backspace textbox in command mode");
                    error!("This should normally be impossible, we already checked it's not empty");
                    return Err(CE::Impossible);
                };
                textbox.draw(true)?;
            },
            'a'..='z' | 'A'..='Z' | '0'..='9' | ' ' => { // Allowed characters
                char_buf.make_ascii_lowercase();
                textbox.append_char(char_buf)?;
                textbox.draw(true)?;
            },
            _ => { // Ignore other characters
                trace!("Ignoring unsupported character received in command mode: {:?} (0x{:X})", char_buf, buf[0]);
                // No need for continue, we just loop again anyway
            },
        }
    }

    // Now we work, knowing we already received the Enter key (because the loop is over)
    let command = textbox.get_text_str()
        .trim(); // Trim all Unicode whitespaces from both ends (including newlines)

    match command {
        "reset" => {
            error!("Resetting microcontroller (command 'reset')");
            cortex_m::peripheral::SCB::sys_reset(); // Reset the microcontroller
        },

        "b" | "bkpt" | "breakpoint" => {
            // Here should be a breakpoint for debugging purposes in your IDE:
            debug!("Breakpoint requested by user (command 'breakpoint')");
        },

        "b alt" | "bkpt alt" | "breakpoint alt" => {
            debug!("Alternative breakpoint requested by user (command 'breakpoint alt')");
            // Will cause an exception if no debugger is attached
            // SAFETY: We know this instruction does not meddle with any registers, and that this is valid assembly, so it has to be safe.
            // By inlining it without a function call, we keep access to local variables if needed for debugging.
            unsafe { core::arch::asm!("bkpt"); } // Inline breakpoint instruction
        },

        "boot usb" | "usb boot" | "usb" => {
            info!("Rebooting into USB bootloader (command 'boot usb')");
            {
                let mut disp = disp_refcell.borrow_mut();
                disp.set_display_on(false)?; // Turns the display off (well, only the grahpics part, it still retains memory) for conventince
            }
            hal::rom_data::reset_to_usb_boot(1 << 25, 0); // Pin 25 for activity LED, both MSC and Picoboot enabled.
        },

        "r" | "f5" | "refresh" | "reload" | "redraw" => {
            info!("Doing a forced redraw of stack. (command 'redraw')");
            stack.draw(true)?; // Just to be sure, we force a flush
        },

        // It is possible to use an array of char-s in starts_with, but not strings, so this is the next best thing.
        // https://stackoverflow.com/a/76964109
        brt_cmd if ["brt", "brightness"].iter().any(|s| brt_cmd.starts_with(*s)) => {
            let split = brt_cmd.rsplit_once(" ")
                .expect("Should contain a space; we checked in the match guard!");

            if split.0 != "brt" && split.0 != "brightness" {
                error!("First part isn't \"brt\" nor \"brightness\", input must've contained multiple spaces.");
                return Err(CE::BadInput);
            }

            let brightness_num = split.1.parse::<u8>()?;
            let brightness = match brightness_num {
                1 => Brightness::DIMMEST,
                2 => Brightness::DIM,
                3 => Brightness::NORMAL,
                4 => Brightness::BRIGHT,
                5 => Brightness::BRIGHTEST,
                _ => {
                    warn!("Brightness value out of range (1-5): {}", brightness_num);
                    return Err(CE::BadInput);
                }
            };
            {
                let mut disp = disp_refcell.borrow_mut();
                disp.set_brightness(brightness)?;
            };
        },

        "c" | "cls" | "clear" => { // We automatically cleared the textbox when switching to command mode
            if stack.is_empty() {
                info!("Stack is already empty, ignoring clear command.");
            } else {
                info!("Clearing stack by user request (command 'clear')");
                stack.clear();
                stack.draw(false)?; // No need to force flush here, we flush after the match block anyway
            }
        },

        "dup" | "duplicate" => {
            if let Some(val) = stack.peek() {
                if stack.push(val.clone()).is_err() {
                    error!("Failed to duplicate top element of stack: CapacityError");
                    return Err(CE::CapacityError);
                };
                stack.draw(false)?;
            } else {
                warn!("Failed to duplicate top element of stack: stack is empty");
                return Err(CE::BadInput);
            }
        },

        drop_cmd if drop_cmd.starts_with("drop ") => {
            // By reverse-splitting instead of splitting, we ensure that in case of multiple spaces,
            // the *first* part would be malformed if there were multiple spaces.
            let split = drop_cmd.rsplit_once(" ")
                .expect("Should contain a space; we checked in the match guard!");

            if split.0 != "drop" {
                error!("First part isn't \"drop\", input must've contained multiple spaces.");
                return Err(CE::BadInput);
            }

            let count = split.1.parse::<usize>()?;
            // This checks if the stack isn't empty as well in sort of a roundabout way
            // (non-zero count will always be greater than stack size if stack is empty)
            if (count == 0) || (count > stack.len()) {
                return Err(CE::BadInput);
            }

            let iter = stack.multipop(count).expect("We already checked if the stack is not empty!");

            // We need this, can't rely on macro debug_assert_eq!()
            #[cfg(debug_assertions)]
            defmt::assert_eq!(iter.count(), count); // Counting the number of stuff popped
            // (consuming the iterator in the process), and asserting that it's as expected.

            // With this attribute, compiler is happy that iterator gets consumed before `draw()` no matter what
            #[cfg(not(debug_assertions))]
            drop(iter); // Automatically pops remaining unconsumed elements without bothering to count them

            stack.draw(false)?; // The cfg-s does not apply to this line anymore
        },

        "drop" => {
            if stack.pop().is_none() {
                warn!("Failed to drop top element of stack: stack is empty.");
                return Err(CE::BadInput);
            };
            stack.draw(false)?;
        },

        "s" | "swap" => {
            // multipop can only fail if there are no elements on the stack,
            // **by design** it will just return fewer elements if there are not enough,
            // so we have to check the stack length ourselves.
            if stack.len() < 2 {
                warn!("Not enough numbers on stack to perform swap. Need 2, got {}.", stack.len());
                return Err(CE::BadInput);
            }

            // Remember, multipop yields elements in reverse order (topmost first)...
            let Some(iter) = stack.multipop(2) else {
                error!("Failed to pop elements from stack for swap.");
                error!("This should be impossible, we already checked that there are at least 2 elements on the stack.");
                return Err(CE::Impossible);
            };

            // We collect the iterator into a Vec first
            // (Cannot push the iterator directly because then we'd have two mutable borrows at once)
            let buf_vec: Vec<T, 2> = iter.collect();
            let Ok(buf) = buf_vec.into_array::<2>() else {
                error!("Failed to collect popped elements into array for swap.");
                error!("This should be impossible, we already checked that we popped exactly 2 elements.");
                return Err(CE::Impossible);
            };

            // ...and push_array pushes them in order (last topmost), swapping them
            if stack.push_array(buf).is_err() {
                error!("Failed to push numbers onto stack: CapacityError");
                error!("This should be impossible, the stack should have enough space since we already popped from it.");
                return Err(CE::Impossible);
            };

            stack.draw(false)?;
        },

        "" => {
            debug!("Ignoring empty command.");
            textbox.draw(true)?;
            {
                let mut disp = disp_refcell.borrow_mut();
                disp.set_invert(false)?;
            }
            return Err(CE::Cancelled);
        },
        
        _ => {
            warn!("Unknown command received over UART: {:?}", command);
            return Err(CE::BadInput);
        }
    }

    {
        let mut disp = disp_refcell.borrow_mut();
        disp.set_invert(false)?;
    }
    
    // Have to clear textbox after handling command because get_text_str() keeps a borrow on it
    textbox.clear();
    textbox.draw(true)?;
    Ok(())
}