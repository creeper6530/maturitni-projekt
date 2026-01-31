#![no_std]
#![no_main]

// 1 MHz, the maximum speed for I²C on the RP2040 (so-called Fast Mode Plus; datasheet 4.3.3), and the SSD1306 can handle it well
const I2C_FREQ: hal::fugit::HertzU32 = hal::fugit::HertzU32::kHz(1000);

// We start RTT in no-blocking mode, `probe-run` will switch to blocking mode.
// Do not disconnect the probe while the program is running, unless you stop probe-run first.
// (Then it will revert to nonblocking: https://github.com/probe-rs/probe-rs/issues/2425)
use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use rp2040_hal::{
    self as hal,
    pac,

    clocks::{
        init_clocks_and_plls,
        Clock // Trait for method `freq()`
    },
    sio::Sio,
    watchdog::Watchdog,
};
use core::cell::RefCell;
use embedded_graphics::{image::Image, prelude::*, pixelcolor::BinaryColor};
use ssd1306::{Ssd1306, mode::BufferedGraphicsMode, prelude::*};
use tinybmp::Bmp;
use heapless::Vec;

mod stack;
use stack::*;
mod textbox;
use textbox::*;
mod decfix;
use decfix::DecimalFixed;
mod custom_error;
use custom_error::{
    CustomError, // Never use `CustomError::*`, it could cause unobvious bugs!
    CE, // Using the type alias from `custom_error.rs`
    IntErrorKindClone as IEKC,
};
mod command_mode;
use command_mode::handle_commands;

// We store the boot2 code in its own section so the linker script can find it
// and place it at the correct address in flash, as required by the RP2040.
#[unsafe(link_section = ".boot2")]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

pub const GRAVE_ERROR_BMP: Result<Bmp<'static, BinaryColor>, tinybmp::ParseError> = Bmp::from_slice(include_bytes!("calc_grave_err.bmp"));
const ERROR_BMP: Result<Bmp<'static, BinaryColor>, tinybmp::ParseError> = Bmp::from_slice(include_bytes!("calc_err.bmp"));

#[inline]
pub fn get_timestamp_us() -> u64 {
    /* Inspired by `https://docs.rs/rp2040-hal/latest/src/rp2040_hal/timer.rs.html#69-88`
    and `https://defmt.ferrous-systems.com/timestamps`, though customised greatly.
    Reading L latches the H register (datasheet section 4.6.2).
    We do disable interrupts, but don't need a full-blown critical section,
    because we don't do dualcore. */

    // We dereference the TIMER peripheral's raw pointer and get a normal reference to it for the methods.
    // Safety: We are guaranteed that the PTR points to a valid place, since we assume the `pac` is infallible.
    let timer_regs = unsafe { &*pac::TIMER::PTR };
    hal::arch::interrupt_free(|| {
        let low: u32 = timer_regs.timelr().read().bits();
        let hi: u32 = timer_regs.timehr().read().bits();
        ((hi as u64) << 32) | (low as u64)
    })
}
defmt::timestamp!("{=u64:us}", { get_timestamp_us() });

#[hal::entry]
fn main() -> ! {
    info!("Program start");
    let mut peri = pac::Peripherals::take().expect("We just booted, so the peripherals should be available.");
    let core = pac::CorePeripherals::take().expect("We just booted, so the core peripherals should be available.");
    let mut watchdog = Watchdog::new(peri.WATCHDOG);
    let sio = Sio::new(peri.SIO);

    let clocks = init_clocks_and_plls(
        12_000_000u32,
        peri.XOSC,
        peri.CLOCKS,
        peri.PLL_SYS,
        peri.PLL_USB,
        &mut peri.RESETS,
        &mut watchdog,
    ).expect("Something went wrong when initializing the clocks.");
    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());
    trace!("Clocks initialized");

    let pins = hal::gpio::Pins::new(
        peri.IO_BANK0,
        peri.PADS_BANK0,
        sio.gpio_bank0,
        &mut peri.RESETS,
    );

    let i2c = hal::I2C::i2c0(
        peri.I2C0,
        pins.gpio8.reconfigure(), // The stuff we're reconfiguring *into* is inferred from the context
        pins.gpio9.reconfigure(),
        I2C_FREQ,
        &mut peri.RESETS,
        &clocks.peripheral_clock,
    );
    trace!("I²C initialized");

    let iface = ssd1306::I2CDisplayInterface::new(i2c);
    let mut disp = Ssd1306::new(iface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    disp.init().expect("Failed to initialize display. Check wiring.");
    disp.set_brightness(Brightness::BRIGHTEST).expect("Failed to set display brightness.");
    trace!("Display initialized");

    // Let me ask one question: Why the hell can't this be as straightforward as I²C is?
    let uart = hal::uart::UartPeripheral::new(
        peri.UART0,
        (pins.gpio0.into_function(), pins.gpio1.into_function()), // Again, inferred from context
        &mut peri.RESETS
    )
    .enable(
        hal::uart::UartConfig::default(), // Default config is a sane 115200 8N1
        clocks.peripheral_clock.freq()
    )
    .expect("Failed to initialize UART peripheral: bad configuration provided.");
    let (rx, tx) = uart.split();
    trace!("UART initialized");

    // Send a message over UART, also clear the terminal (VT100 codes)
    tx.write_full_blocking(b"\x1b[2J\x1b[HUART initialised!\r\n");

    // ----------------------------------------------------------------------------

    let disp_refcell = RefCell::new(disp);
    let mut stack: CustomStack<'_, DecimalFixed, _, _> = CustomStackBuilder::new() // Specifying DecimalFixed just to be sure
        .build(&disp_refcell);
    let mut textbox = CustomTextboxBuilder::new()
        .build(&disp_refcell);

    // We can't very well draw an error indication on the display if the display is not working, nay?
    // That's why we're panicking on error here, and everywhere else where we have possible display errors.
    stack.draw(false).expect("Error with display");
    textbox.draw(true).expect("Error with display");

    tx.write_full_blocking(b"Entering main loop\r\n");
    info!("Entering main loop");

    // Label the main loop so we can call `continue` simpler-ly (more simply?) in case of errors if there were nested loops.
    'main: loop {
        // Due to making the buffer only one byte large, we read **one** byte at a time. Most of our input is ASCII anyway.
        let mut buf: [u8; 1] = [0]; // Yes, we do need to initialize it even if we overwrite it immediately.
        if let Err(e) = rx.read_full_blocking(&mut buf) {
            error!("Failed to read from UART: {:?}", e);
            if let hal::uart::ReadErrorType::Break = e { // ReadErrorType does not implement PartialEq
                debug!("Check wiring, usually a break indicates a disconnected wire at the RX pin.");
            };

            disp_error(&disp_refcell);
            warn!("Delaying for a second before trying to read again");
            delay.delay_ms(1000); // Wait a second before trying again, to avoid spamming the error indication
            continue 'main;
        }

        let Some(char_buf) = char::from_u32(buf[0] as u32) else {
            warn!("Received invalid UTF-8 byte over UART: 0x{:X}, continuing the loop", buf[0]);
            continue 'main;
        };

        match char_buf {
            '\r' | '\n' => { // Enter or newline
                if textbox.is_empty() || textbox.get_text_str() == "-" {
                    continue 'main; // Ignore empty textbox or textbox with just a minus sign, continuing
                }

                if let Err(e) = parse_textbox(&mut textbox, &mut stack, true) {
                    match e {
                        CE::CapacityError |
                        CE::MathOverflow |
                        CE::ParseIntError(IEKC::PosOverflow | IEKC::NegOverflow) => {
                            error!("Error parsing textbox: {:?}", e);
                            stack.draw(false).expect("Error with display");
                            textbox.draw(true).expect("Error with display");

                            disp_error(&disp_refcell);
                        },
                        CE::DisplayError(e) => defmt::panic!("Error with display: {:?}", e),
                        _ => disp_grave_error(&disp_refcell, Some(&mut delay))
                    }
                }
            },

            '\x08' | '\x7F' => { // Backspace or Delete
                trace!("Backspace character received: (0x{:X})", buf[0]);

                if textbox.is_empty() {
                    continue 'main;
                };
                if textbox.backspace(1).is_err() {
                    error!("Failed to backspace textbox");
                    error!("This should normally be impossible, we already checked it's not empty");
                    disp_grave_error(&disp_refcell, Some(&mut delay));
                };
                textbox.draw(true).expect("Error with display");
            },

            '.' | ',' => { // Decimal point
                if textbox.is_empty() {
                    if textbox.append_str("0.").is_err() {
                        error!("It should be impossible to fail to append to an empty textbox.");
                        disp_grave_error(&disp_refcell, Some(&mut delay));
                    }
                    textbox.draw(true).expect("Error with display");
                    continue 'main;
                }
                if textbox.contains('.') {
                    debug!("Ignoring decimal point, textbox already contains one");
                    continue 'main;
                }
                if textbox.append_char('.').is_err() {
                    error!("Failed to append decimal point to textbox: CapacityError");
                    disp_error(&disp_refcell);
                    continue 'main;
                }
                textbox.draw(true).expect("Error with display");
            },

            'n' => { // Negate
                if textbox.is_empty() {
                    if textbox.append_char('-').is_err() {
                        error!("It should be impossible to fail to append to an empty textbox.");
                        disp_grave_error(&disp_refcell, Some(&mut delay));
                    }
                    textbox.draw(true).expect("Error with display");
                } else if textbox.starts_with('-') {
                    match textbox.remove_at(0) {
                        Ok('-') => { // Good result
                            textbox.draw(true).expect("Error with display");
                            continue 'main;
                        },
                        Ok(other) => { // Popped something else, despite our check
                            error!("Removed character was not '-' ({:?}), this should be impossible!", other);
                            disp_grave_error(&disp_refcell, Some(&mut delay));
                        },
                        Err(e) => { // Failed to remove
                            error!("Failed to remove leading '-' from textbox: {:?}", e);
                            disp_grave_error(&disp_refcell, Some(&mut delay));
                        }
                    };
                } else if textbox.contains('-') {
                    error!("Textbox contains '-' not at the start, this should be impossible.");
                    disp_grave_error(&disp_refcell, Some(&mut delay));
                } else {
                    if let Err(e) = textbox.insert_at(0, '-') {
                        error!("Failed to insert leading '-' into textbox: {:?}", e);
                        disp_error(&disp_refcell);
                    };
                    
                    textbox.draw(true).expect("Error with display");
                }
            },

            '0'..='9' => { // Digits
                if textbox.append_char(char_buf).is_err() {
                    disp_error(&disp_refcell);
                    continue 'main;
                };
                textbox.draw(true).expect("Error with display");
            }

            '+' | '-' | '*' | '/' => {
                // The short-circuiting is desirable: if it's empty, we never run `parse_textbox()`
                if !textbox.is_empty()
                    && let Err(e) = parse_textbox(&mut textbox, &mut stack, false)
                {
                    match e {
                        CE::CapacityError |
                        CE::MathOverflow |
                        CE::ParseIntError(IEKC::PosOverflow | IEKC::NegOverflow) => {
                            error!("Error parsing textbox: {:?}", e);
                            stack.draw(false).expect("Error with display");
                            textbox.draw(true).expect("Error with display");
                            disp_error(&disp_refcell);
                        },
                        CE::DisplayError(e) => defmt::panic!("Error with display: {:?}", e),
                        _ => disp_grave_error(&disp_refcell, Some(&mut delay))
                    };
                    continue 'main;
                }

                if stack.len() < 2 {
                    warn!("Not enough numbers on stack to perform operation. Need 2, got {}.", stack.len());
                    disp_error(&disp_refcell);
                    continue 'main;
                }
                // By definition of multipop, the first popped element is the topmost one,
                // but we want the pushed-later one to be the second operand (B),
                // so we reverse the order here. For example, for "5 6 -", we do 5 - 6, not 6 - 5
                // An easy test is to try "10 0 /" and see if it errors out.
                let [b, a] = stack.multipop(2)
                    .expect("We already checked the stack isn't empty!")
                    .collect::<Vec<_, 2>>() // Collect the iterator into a collection
                    .into_array() // Convert the collection into a const-size array
                    .expect("We already checked the stack has at least 2 elements, the collected Vec should have 2 items!");

                let c: DecimalFixed = match char_buf {
                    '+' => match a + b {
                        Ok(c) => c,
                        Err(e) => {
                            error!("Error in addition: {:?}", e);
                            stack.draw(false).expect("Error with display");
                            disp_error(&disp_refcell);
                            continue 'main;
                        }
                    },
                    '-' => match a - b {
                        Ok(c) => c,
                        Err(e) => {
                            error!("Error in subtraction: {:?}", e);
                            stack.draw(false).expect("Error with display");
                            disp_error(&disp_refcell);
                            continue 'main;
                        }
                    },
                    '*' => {
                        match a * b {
                            Ok(c) => c,
                            Err(e) => {
                                error!("Error in multiplication: {:?}", e);
                                stack.draw(false).expect("Error with display");
                                disp_error(&disp_refcell);
                                continue 'main;
                            }
                        }
                    }
                    '/' => {
                        if b.is_zero() {
                            error!("Division by zero attempted.");
                            stack.draw(false).expect("Error with display");
                            disp_error(&disp_refcell);
                            continue 'main;
                        };

                        match a / b {
                            Ok(c) => c,
                            Err(e) => {
                                error!("Error in division: {:?}", e);
                                stack.draw(false).expect("Error with display");
                                disp_error(&disp_refcell);
                                continue 'main;
                            }
                        }
                    },
                    _ => defmt::unreachable!(), // We already checked this above
                };

                if stack.push(c).is_ok() {
                    stack.draw(false).expect("Error with display");
                    textbox.draw(true).expect("Error with display");
                } else {
                    error!("Failed to push result onto stack");
                    error!("This should be impossible, the stack should have enough space since we already popped from it.");
                    disp_grave_error(&disp_refcell, Some(&mut delay));
                };
            },

            '\x12' => { // Ctrl-R
                // Force a redraw of both textbox and stack
                // Amongst other effects, this clears the non-grave error icon
                info!("Doing a forced redraw of both stack and textbox.");
                
                // Just to be ultra-sure, we flush both
                stack.draw(true).expect("Error with display");
                textbox.draw(true).expect("Error with display");
            },

            '\x14' => { // Ctrl-T
                match handle_commands(&rx, &disp_refcell, &mut textbox, &mut stack) {
                    Ok(()) => {},
                    Err(e) => {
                        match e {
                            CE::BadInput |
                            CE::ParseIntError(_) |
                            CE::CapacityError => {
                                {
                                    let mut disp = disp_refcell.borrow_mut();
                                    disp.set_invert(false).expect("Failed to invert display");
                                }

                                textbox.clear();
                                stack.draw(false).expect("Error with display");
                                textbox.draw(false).expect("Error with display");

                                disp_error(&disp_refcell);
                            },
                            CE::Cancelled => { // Not truly an error, just a notification
                                info!("Command mode cancelled by user.");
                                textbox.draw(true).expect("Error with display");
                            },
                            CE::DisplayError(e) => defmt::panic!("Error with display: {:?}", e),
                            other => {
                                error!("An irrecoverable or otherwise unhandled error: {:?}", other);
                                {
                                    let mut disp = disp_refcell.borrow_mut();
                                    disp.set_invert(false).expect("Failed to invert display");
                                }
                                disp_grave_error(&disp_refcell, Some(&mut delay));
                            }
                        }
                    }
                };
            },

            '\x1B' => { // Escape character - start of an escape sequence
                // We read 10 bytes to "flush" the input so that we don't read it in the next loop iterations.
                let mut buf = [0_u8; 11];
                buf[0] = 0x1B; // We already read the first byte, so store it

                delay.delay_ms(50); // HACK: Wait a bit to allow the rest of the sequence to arrive.
                let Ok(num_bytes) = rx.read_raw(&mut buf[1..]) else { // Nonblocking
                    debug!("Escape byte received over UART: 0x1B");
                    continue 'main;
                };

                // We do not handle the escape sequences at all, just log them for debugging purposes.
                debug!("Escape sequence received over UART: {:#04X}", buf[..=num_bytes]); // With the RangeToInclusive, we account for the first byte
                continue 'main;
            },

            _ => {
                warn!("Unhandled character received over UART: {:?} ({:#04X})", char_buf, buf[0]);
                continue 'main;
            },
        }
    };
}


/// Display the grave error image and reset the microcontroller after a delay, never returning.
pub fn disp_grave_error<DI, SIZE>(
    disp_refcell: &RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,
    maybe_delay: Option<&mut cortex_m::delay::Delay>
) -> !
where 
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    let mut disp = disp_refcell.borrow_mut();

    // Converted at https://convertico.com/png-to-bmp/ to 1-bit BMP
    let bmp = GRAVE_ERROR_BMP
        .expect("Failed to load grave error image from memory. Image data must be malformed.");
    let img = Image::new(
        &bmp,
        (0, 0).into(), // Fullscreen
    );
    img.draw(&mut (*disp)).expect("Failed to draw image on display");
    // The dereference gives us the inner Ssd1306 struct from the RefCell,
    // and then we borrow it mutably to draw on it.
    // We could also do `disp.deref_mut()` instead of `&mut (*disp)`.
    disp.flush().expect("Failed to flush display");

    maybe_delay.expect("No delay provider given, cannot delay before reset. Panicking.")
        .delay_ms(10_000);
    cortex_m::peripheral::SCB::sys_reset(); // Reset the microcontroller
}

// Display the non-grave error image (on top-right corner) and return.
pub fn disp_error<DI, SIZE> (
    disp_refcell: &RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,
) where
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    let mut disp = disp_refcell.borrow_mut();

    let bmp = ERROR_BMP
        .expect("Failed to load error image from memory. Image data must be malformed.");
    let img = Image::new(
        &bmp,
        (117, 0).into(), // Image is 10x10, we put it in the top-right corner
    );
    img.draw(&mut (*disp)).expect("Failed to draw image on display");
    disp.flush().expect("Failed to flush display");
}

// The stack is intentionally not generic, only for DecimalFixed
// XXX: Will need a rewrite if the stack type changes, since we can't impl FromStr with static exp
pub fn parse_textbox<'a, DI, SIZE> (
    textbox: &mut CustomTextbox<'a, DI, SIZE>,
    stack: &mut CustomStack<'a, DecimalFixed, DI, SIZE>,
    flush: bool,
) -> Result<(), CustomError>
where
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    let txbx_data = textbox.get_text_str();
    if txbx_data.is_empty() { return Err(CE::BadInput); };
    
    let num = DecimalFixed::parse_str(txbx_data, None)?; // Use default exponent by passing None
    match stack.push(num) {
        Ok(()) => {},
        Err((e, _)) => { // We drop the returned value, we don't need it
            // .push() will only return CE::CapacityError
            error!("Failed to push parsed number onto stack (CapacityError)");
            return Err(e)
        },
    }

    textbox.clear();
    // We save ourselves a double flush call when drawing both, because I²C ops are slow and blocking
    stack.draw(false)?;
    textbox.draw(flush)?;
    Ok(())
}
