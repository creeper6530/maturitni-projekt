use defmt::*;
use rp2040_hal as hal;

use embedded_graphics::{image::Image, prelude::*};
use ssd1306::{prelude::*, Ssd1306, mode::BufferedGraphicsMode};
use tinybmp::Bmp;

use core::cell::RefCell;
use core::ops::DerefMut;

use crate::textbox::CustomTextbox;
use crate::stack::CustomStack;
use crate::custom_error::CustomError;

/// # List of commands:
/// 
/// - `reset`: Reset the microcontroller
/// - `halt`: Halt the microcontroller (actually causes a hard fault by executing an undefined instruction)
/// - `breakpoint` (aliases: `bkpt`, `b`): Trigger a breakpoint set in your debugger/IDE
/// - `breakpoint alt` (aliases: `bkpt alt`, `b alt`): Trigger an inline breakpoint instruction (causes exception if no debugger attached)
/// - `boot usb` (aliases: `usb boot`, `usb`): Reboot into the USB bootloader
/// - `redraw` (aliases: `refresh`, `reload`, `r`, `f5`): Force a redraw of both stack and textbox
///   - Also can be triggered by pressing F5 or Ctrl-R (technically sending the VT100-style escape codes for those keys)
/// - `brightness N` (aliases: `brt N`): Set display brightness to a predefined level between 1 and 5
/// - `clear` (aliases: `cls`, `c`): Clear the stack
/// - `duplicate` (aliases: `dup`, `d`): Duplicate the top element of the stack
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
    T: Copy + core::fmt::Debug + core::fmt::Display,

    D: hal::uart::UartDevice,
    P: hal::uart::ValidUartPinout<D>
{
    info!("Entering command mode");
    textbox.clear();
    textbox.draw(true)?;

    { // We limit the scope of the mutable borrow, so that it doesn't panic when display is dropped.
        let mut disp = disp_refcell.borrow_mut();
        disp.set_invert(true)?;
    }   

    let mut buf: [u8; 1] = [0];
    let mut char_buf: char; // Uninitialised because we don't read it before it's first written to, but we don't need constant stack allocations.
    loop {
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
                return Err(CustomError::Cancelled);
            },
            '\r' | '\n' => break, // Enter key
            '\x08' | '\x7F' => { // Backspace
                trace!("Backspace character received in command mode: (0x{:X})", buf[0]);

                if textbox.is_empty() {
                    info!("Ignoring backspace on empty textbox in command mode.");
                    continue; // Diverging, does not continue forwards
                };
                if textbox.backspace(1).is_err() {
                    error!("Failed to backspace textbox in command mode");
                    error!("This should normally be impossible, we already checked it's not empty");
                    return Err(CustomError::Impossible);
                };
                textbox.draw(true)?;
            },
            'a'..='z' | '0'..='9' | ' ' => {
                textbox.append_char(char_buf)?;
                textbox.draw(true)?;
            },
            _ => { // Ignore other characters
                trace!("Ignoring unsupported character received in command mode: {:?} (0x{:X})", char_buf, buf[0]);
                // No need for continue, we just loop again anyway
            },
        }
    }

    // Now we work, knowing we already received the Enter key
    let command = textbox.get_text();
    let command = command.as_str(); // Temporary value dropped while still borrowed... fuck. Have to shadow.
    textbox.clear();
    textbox.draw(true)?;
    // We don't need to pop the last char, since we didn't add it to the textbox

    match command {
        "reset" => {
            info!("Resetting microcontroller (command 'reset')");
            cortex_m::peripheral::SCB::sys_reset(); // Reset the microcontroller
        },

        "halt" => {
            error!("Halting microcontroller (command 'halt')");
            halt(&disp_refcell);
        },

        "b" | "bkpt" | "breakpoint" => {
            // Here should be a breakpoint for debugging purposes in your IDE:
            debug!("Breakpoint requested by user (command 'breakpoint')");
        },

        "b alt" | "bkpt alt" | "breakpoint alt" => {
            debug!("Alternative breakpoint requested by user (command 'breakpoint alt')");
            // Will cause an exception if no debugger is attached
            cortex_m::asm::bkpt(); // Inline breakpoint instruction
        },

        "boot usb" | "usb boot" | "usb" => {
            info!("Rebooting intto USB bootloader (command 'boot usb')");
            {
                let mut disp = disp_refcell.borrow_mut();
                disp.set_display_on(false)?;
            }
            hal::rom_data::reset_to_usb_boot(1 << 25, 0); // Pin 25 for activity LED, both MSC and Picoboot enabled.
        },

        "r" | "f5" | "refresh" | "reload" | "redraw" => {
            info!("Doing a forced redraw of both stack and textbox. (command 'redraw')");
            stack.draw(true)?;
            textbox.draw(true)?;
        },

        brt_cmd if brt_cmd.starts_with("brt ")
            || brt_cmd.starts_with("brightness ") => {
            let split = brt_cmd.rsplit_once(" ")
                .expect("Should contain a space; we checked in the match guard!");

            if split.0 != "brt" && split.0 != "brightness" {
                error!("We already checked the command starts with 'brt ' or 'brightness ', why is the first split part not one of those?.
Must've contained multiple spaces.");
                return Err(CustomError::BadInput);
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
                    return Err(CustomError::BadInput);
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
                stack.draw(true)?;
            }
        },

        "d" | "dup" | "duplicate" => {
            if let Some(val) = stack.pop() {
                for _ in 0..2 { // Two times
                    stack.push(val)?;
                }
                stack.draw(true)?;
            } else {
                warn!("Failed to duplicate top element of stack: stack is empty");
                return Err(CustomError::BadInput);
            }
        },

        drop_cmd if drop_cmd.starts_with("drop ") => {
            let split = drop_cmd.rsplit_once(" ")
                .expect("Should contain a space; we checked in the match guard!");

            if split.0 != "drop" {
                error!("We already checked the command starts with 'drop ', why is the first split part not 'drop'?.
Must've contained multiple spaces.");
                return Err(CustomError::BadInput);
            }

            let count = split.1.parse::<u8>()?;
            if (count == 0) || (count as usize > stack.len()) {
                return Err(CustomError::BadInput);
            }

            stack.multipop(count).ok_or(CustomError::BadInput)?;
            stack.draw(true)?;
        },

        "drop" => {
            if stack.pop().is_none() {
                warn!("Failed to drop top element of stack: stack is empty.");
                return Err(CustomError::BadInput);
            };
            stack.draw(true)?;
        },

        "s" | "swap" => {
            // B was pushed later, so it is popped first
            let option_b = stack.pop();
            let option_a = stack.pop();

            match (option_a, option_b) {
                (Some(a), Some(b)) => {
                    // Earlier B was pushed later, so it is now pushed first
                    // Evaluation order is defined left-to-right
                    // We do NOT want to use short-circuiting here
                    if stack.push(b).is_err() | stack.push(a).is_err() {
                        error!("Failed to push number onto stack.");
                        error!("This should be impossible, the stack should have enough space since we already popped from it.");
                        return Err(CustomError::Impossible);
                    };
                    stack.draw(true)?;
                },
                (None, Some(a)) => {
                    warn!("Failed to swap top two elements of stack: stack has only one element.");
                    if let Err(e) = stack.push(a) {
                        error!("Failed to push number onto stack: {:?}", e);
                        error!("This should be impossible, the stack should have enough space since we already popped from it.");
                        return Err(CustomError::Impossible);
                    }
                    return Err(CustomError::BadInput);
                },
                (None, None) => {
                    warn!("Failed to swap top two elements of stack: stack is empty.");
                    return Err(CustomError::BadInput);
                },
                (Some(_), None) => {
                    error!("This should be impossible. How can we first fail but then succeed?");
                    return Err(CustomError::Impossible);
                },
            };
        },

        "" => {
            debug!("Ignoring empty command.");
            textbox.draw(true)?;
            {
                let mut disp = disp_refcell.borrow_mut();
                disp.set_invert(false)?;
            }
            return Err(CustomError::Cancelled);
        },
        
        _ => {
            warn!("Unknown command received over UART: {:?}", command);
            return Err(CustomError::BadInput);
        }
    }

    {
        let mut disp = disp_refcell.borrow_mut();
        disp.set_invert(false)?;
    }
    textbox.draw(true)?;
    Ok(())
}

fn halt<'a, DI, SIZE>(
    disp_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>
) -> !
where 
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    let mut disp = disp_refcell.borrow_mut();

    // Converted at https://convertico.com/png-to-bmp/ to 1-bit BMP
    let bmp = Bmp::from_slice(include_bytes!("calc_grave_err.bmp")).expect("Failed to load grave error image from memory. Image data must be malformed.");
    let img = Image::new(
        &bmp,
        (0, 0).into(), // Fullscreen
    );
    if let Err(e) = img.draw(disp.deref_mut()) {
        // We can't use `defmt::panic!()` here because DisplayError does not implement `defmt::Format`
        core::panic!("Failed to draw image on display: {:?}", e);
    };
    if let Err(e) = disp.flush() {
        core::panic!("Failed to flush display: {:?}", e);
    };
    
    cortex_m::asm::udf(); // Undefined instruction to cause a hard fault
}