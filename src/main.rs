//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

const I2C_FREQ_KHZ: u32 = 1000; // 1 MHz, the maximum speed for I²C on the RP2040 (so-called Fast Mode Plus; datasheet 4.3.3), and the SSD1306 can handle it well

use defmt::*;
use defmt_rtt as _; // We start RTT in no-blocking mode, `probe-run` will switch to blocking mode. That's why we shall not disconnect the probe while the program is running.
use panic_probe as _;

use rp2040_hal as hal;
use hal::{
    pac,

    clocks::{Clock, init_clocks_and_plls},
    watchdog::Watchdog,
    
    sio::Sio,

    dma::single_buffer::Config as DmaSingleBufferConfig,
    dma::DMAExt,
};
use rp2040_hal::fugit::RateExtU32; // For the `.kHz()` method on u32 integers

// Display imports
use embedded_graphics::{image::Image, prelude::*};
use ssd1306::{prelude::*, Ssd1306, mode::BufferedGraphicsMode};
use tinybmp::Bmp;

use core::cell::RefCell;
use core::ops::DerefMut;

mod stack;
use stack::*;
mod textbox;
use textbox::*;
mod decfix;
use decfix::DecimalFixed;
mod custom_error;
use custom_error::CustomError::{self, *};

const DECFIX_EXPONENT: i8 = -9; // We use 9 decimal places, which is enough for most calculations

#[unsafe(link_section = ".boot2")]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

defmt::timestamp!("{=u64:us}", {
    /* Stolen from `https://docs.rs/rp2040-hal/latest/src/rp2040_hal/timer.rs.html#69-88`
    and `https://defmt.ferrous-systems.com/timestamps`, though customised greatly.
    We use the critical section to ensure no disruptions, because reading L latches the H register (datasheet section 4.6.2)
    It could have unforseen consequences if we try reading again while there's already a read in progress. */

    // Safety: We are guaranteed that the PTR points to a valid place, since we assume the `pac` is infallible.
    let timer_regs = unsafe { &*pac::TIMER::PTR }; // We dereference the TIMER peripheral's raw pointer and get a normal reference to it.
    critical_section::with(|_| {
        let low: u32 = timer_regs.timelr().read().bits();
        let hi: u32 = timer_regs.timehr().read().bits();
        ((hi as u64) << 32) | (low as u64)
    })
});

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
        pins.gpio8.reconfigure(),
        pins.gpio9.reconfigure(),
        I2C_FREQ_KHZ.kHz(),
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
        (pins.gpio0.into_function(), pins.gpio1.into_function()), // Luckily the function itself is inferred, so we don't need to specify it explicitly
        &mut peri.RESETS
    )
    .enable(hal::uart::UartConfig::default(), clocks.peripheral_clock.freq()) // Default is a sane 115200 8N1
    .expect("Failed to initialize UART peripheral: bad configuration provided.");
    let (rx, tx) = uart.split();
    trace!("UART initialized");
    
    // Here we're basically just flexing that we can use DMA :D
    let dma = peri.DMA.split(&mut peri.RESETS);
    let tx_transfer = DmaSingleBufferConfig::new(dma.ch0, b"\x1b[2J\x1b[HUART initialised!\r\n", tx).start(); // Send a message over UART using DMA, also clear the terminal (VT100 codes)

    // ----------------------------------------------------------------------------

    let disp_refcell: RefCell<Ssd1306<I2CInterface<rp2040_hal::I2C<pac::I2C0, (rp2040_hal::gpio::Pin<rp2040_hal::gpio::bank0::Gpio8, rp2040_hal::gpio::FunctionI2c, rp2040_hal::gpio::PullUp>, rp2040_hal::gpio::Pin<rp2040_hal::gpio::bank0::Gpio9, rp2040_hal::gpio::FunctionI2c, rp2040_hal::gpio::PullUp>)>>, DisplaySize128x64, ssd1306::mode::BufferedGraphicsMode<DisplaySize128x64>>> = RefCell::new(disp);
    // Range of i32 is `-2147483648..=2147483647`
    let mut stack: CustomStack<'_, DecimalFixed, _, _> = CustomStackBuilder::<'_, DecimalFixed, _, _>::new(&disp_refcell) // We're using the turbofish syntax here
        .build();
    let mut textbox: _ = CustomTextboxBuilder::new(&disp_refcell)
        .build();

    stack.draw(false)
        .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
        .unwrap(); // Safe since the error would panic
    textbox.draw(true)
        .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
        .unwrap(); // Safe since the error would panic

    trace!("Waiting for initial DMA transfer to complete, should be instant");
    let (ch0, _, tx) = tx_transfer.wait(); // So that we can reuse them. We don't really care about reclaiming the &'static buffer tho, so we ignore it
    trace!("Finished waiting");
    let _new_tx_transfer = DmaSingleBufferConfig::new(ch0, b"Entering main loop\r\n", tx).start(); // Send another message with DMA, this time we don't need to reclaim the channel, so we don't wait for it to finish
    info!("Entering main loop");

    loop {
        let mut buf: [u8; 1] = [0];
        if let Err(e) = rx.read_full_blocking(&mut buf) { // TODO: Figure out a way to do this non-blocking, perhaps with DMA and/or an interrupt. I tried and failed miserably. Maybe I should just have used an async executor like Embassy.
            error!("Failed to read from UART: {:?}", e);
            if let hal::uart::ReadErrorType::Break = e {
                debug!("Check wiring, usually a break indicates a disconnected wire at the RX pin.");
            };

            disp_error(&disp_refcell);
            warn!("Delaying for a second before trying to read again");
            delay.delay_ms(1000); // Wait a second before trying again, to avoid spamming the error indication
            continue;
        }

        let char_buf = char::from_u32(buf[0] as u32).unwrap_or('?'); // Replace invalid UTF-8 with a replacement character

        match char_buf {
            '\r' | '\n' => { // Enter or newline
                if textbox.is_empty() {
                    continue; // Ignore empty textbox
                }

                if let Err(e) = parse_textbox(&mut textbox, &mut stack, true) {
                    match e {
                        CapacityError |
                        MathOverflow => {
                            stack.draw(false)
                                .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                                .unwrap();
                            textbox.draw(true)
                                .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                                .unwrap();

                            disp_error(&disp_refcell);
                        },
                        DisplayError(e) => defmt::panic!("Error with display: {:?}", e),
                        _ => disp_grave_error(&disp_refcell, Some(&mut delay))
                    }
                } // Else it's already drawn
            },

            '\u{8}' | '\u{7F}' => { // Backspace or Delete
                #[cfg(debug_assertions)]
                debug!("Backspace character received: (0x{:X})", buf[0]);

                if textbox.is_empty() {
                    info!("Ignoring backspace on empty textbox.");
                    continue; // Diverging, does not continue forwards
                };
                if textbox.backspace(1).is_err() {
                    error!("Failed to backspace textbox");
                    error!("This should normally be impossible, we already checked it's not empty");
                    disp_grave_error(&disp_refcell, Some(&mut delay)); // Diverging too
                };
                textbox.draw(true)
                    .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                    .unwrap();

            },

            '.' | ',' => { // Decimal point
                if textbox.is_empty() {
                    if textbox.append_str("0.").is_err() {
                        error!("It should be impossible to fail to append to an empty textbox.");
                        disp_grave_error(&disp_refcell, Some(&mut delay));
                    }
                    textbox.draw(true)
                        .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                        .unwrap();
                    continue;
                }
                if textbox.contains('.') {
                    info!("Ignoring decimal point, textbox already contains one");
                    continue;
                }
                if textbox.append_char('.').is_err() {
                    // All the warnings were already emitted in the `append_char()` method
                    disp_error(&disp_refcell);
                    continue;
                }
                textbox.draw(true)
                    .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                    .unwrap();
            },

            '0'..='9' => { // Digits
                if textbox.append_char(char_buf).is_err() {
                    disp_error(&disp_refcell);
                    continue;
                };
                textbox.draw(true)
                    .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                    .unwrap();
            },

            '+' | '-' | '*' | '/' => {
                if !textbox.is_empty() {
                    if let Err(e) = parse_textbox(&mut textbox, &mut stack, false) {
                        match e {
                        CapacityError |
                        MathOverflow => {
                            stack.draw(false)
                                .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                                .unwrap();
                            textbox.draw(true)
                                .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                                .unwrap();

                            disp_error(&disp_refcell);
                        },
                        DisplayError(e) => defmt::panic!("Error with display: {:?}", e),
                        _ => disp_grave_error(&disp_refcell, Some(&mut delay))
                        };
                        continue;
                    } // Else it's already drawn
                }

                // Since the stack is LIFO, the A is the one pushed earlier, so it is popped later
                // So that for "5 6 -", we do 5 - 6, not 6 - 5
                let b_res = stack.pop();
                let a_res = stack.pop();

                let c: DecimalFixed = match (a_res, b_res) {
                    (Some(a), Some(b)) => {
                        match char_buf {
                            '+' => a + b,
                            '-' => a - b,
                            '*' => {
                                let Ok(c) = a.multiply(b) else {
                                    error!("Multiplication errored (overflow?)");
                                    stack.draw(false)
                                        .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                                        .unwrap();
                                    disp_error(&disp_refcell);
                                    continue;
                                };
                                c
                            }
                            '/' => {
                                if b.is_zero() {
                                    error!("Division by zero");
                                    stack.draw(false)
                                        .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                                        .unwrap();
                                    disp_error(&disp_refcell);
                                    continue;
                                } else {
                                    let Ok(c) = a.divide(b) else {
                                        error!("Division errored (overflow?)");
                                        stack.draw(false)
                                            .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                                            .unwrap();
                                        disp_error(&disp_refcell);
                                        continue;
                                    };
                                    c
                                }
                            },
                            _ => defmt::unreachable!(), // We already checked this above
                        }
                    },

                    (None, Some(_)) | (None, None) => {
                        // TODO: Perhaps push the one popped number back like in swap?
                        error!("Failed to pop number from stack");
                        stack.draw(false) // Redraw stack because we popped something
                            .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                            .unwrap();
                        disp_error(&disp_refcell); // Must be after stack is drawn in order not to be overdrawn. Always flushes.
                        continue;
                    },

                    (Some(_), None) => {
                        error!("This should be impossible. How can we first fail but then succeed?");
                        disp_grave_error(&disp_refcell, Some(&mut delay));
                    }
                };

                if stack.push(c).is_ok() {
                    stack.draw(false)
                        .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                        .unwrap();
                    textbox.draw(true)
                        .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                        .unwrap();
                } else {
                    error!("Failed to push result onto stack");
                    error!("This should be impossible, the stack should have enough space since we already popped from it.");
                    disp_grave_error(&disp_refcell, Some(&mut delay));
                };
            },

            'c' => { // Clear textbox
                if textbox.is_empty() {
                    info!("Textbox is empty. Ignoring.");
                    continue;
                }
                textbox.clear();
                textbox.draw(true)
                    .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                    .unwrap();
            },

            'C' => { // Clear everything (we assume the Shift-C is enough of a modifier)
                match (textbox.is_empty(), stack.is_empty()) {
                    (true, true) => {
                        info!("Both stack and textbox are empty. Ignoring both.");
                    },
                    (true, false) => {
                        info!("Textbox is empty. Ignoring it.");
                        stack.clear();
                        stack.draw(true)
                            .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                            .unwrap();
                    },
                    (false, true) => {
                        info!("Stack is empty. Ignoring it.");
                        textbox.clear();
                        textbox.draw(true)
                            .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                            .unwrap();
                    },
                    (false, false) => {
                        textbox.clear();
                        stack.clear();
                        textbox.draw(false)
                            .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                            .unwrap();
                        stack.draw(true)
                            .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                            .unwrap();
                    }
                }
            },

            'd' => { // Duplicate the top element of the stack
                if let Some(val) = stack.pop() {
                    for _ in 0..2 { // Two times
                        if let Err(e) = stack.push(val) {
                            error!("Failed to push number onto stack: {:?}", e);
                            disp_error(&disp_refcell);
                            continue;
                        }
                    }
                    stack.draw(true)
                        .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                        .unwrap();
                } else {
                    warn!("Failed to duplicate top element of stack: stack is empty");
                    disp_error(&disp_refcell);
                    continue;
                }
            },

            'D' => { // Drop the topmost element of the stack
                if stack.pop().is_none() {
                    warn!("Failed to drop top element of stack: stack is empty.");
                    disp_error(&disp_refcell);
                    continue;
                };
                stack.draw(true)
                    .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                    .unwrap();
            },

            's' => { // Swap the top two elements of the stack
                // B was pushed later, so it is popped first
                let option_b = stack.pop();
                let option_a = stack.pop();

                match (option_a, option_b) {
                    (Some(a), Some(b)) => {
                        // Earlier B was pushed later, so it is now pushed first
                        if let Err(e) = stack.push(b) {
                            error!("Failed to push number onto stack: {:?}.", e);
                            error!("This should be impossible, the stack should have enough space since we already popped from it.");
                            disp_grave_error(&disp_refcell, Some(&mut delay));
                        };
                        if let Err(e) = stack.push(a) {
                            error!("Failed to push number onto stack: {:?}", e);
                            error!("This should be impossible, the stack should have enough space since we already popped from it.");
                            disp_grave_error(&disp_refcell, Some(&mut delay));
                        };
                        stack.draw(true)
                            .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                            .unwrap();
                    },
                    (None, Some(a)) => {
                        warn!("Failed to swap top two elements of stack: stack has only one element.");
                        if let Err(e) = stack.push(a) {
                            error!("Failed to push number onto stack: {:?}", e);
                            error!("This should be impossible, the stack should have enough space since we already popped from it.");
                            disp_grave_error(&disp_refcell, Some(&mut delay));
                        }
                    },
                    (None, None) => {
                        warn!("Failed to swap top two elements of stack: stack is empty.");
                        disp_error(&disp_refcell)
                    },
                    (Some(_), None) => {
                        error!("This should be impossible. How can we first fail but then succeed?");
                        disp_grave_error(&disp_refcell, Some(&mut delay));
                    },
                };
            },

            '\x03' => { // Ctrl-C
                error!("Stopped by user (Ctrl-C)");
                disp_grave_error(&disp_refcell, None); // We don't reset, just panic.
            },

            '\x1B' => { // Escape character
                let mut buf = [0_u8; 6]; // We read 6 bytes, because the escape sequence is usually up to 6 bytes long (not for all implementations, but for most common ones it is)
                delay.delay_ms(50); // HACK: Wait a bit to allow the rest of the sequence to arrive.
                let _ = rx.read_raw(&mut buf); // We ignore the result, since Ctrl-[ (that's Ctrl+ú on a Czech keyboard) produces only the escape character

                // TODO: Maybe even move the cursor around in the textbox?

                // See https://en.wikipedia.org/wiki/ANSI_escape_code?useskin=vector#Terminal_input_sequences for list of (common) escape sequences
                match buf {
                    [b'[', b'A', ..] => { // Up arrow
                        info!("Up arrow pressed");
                    },
                    [b'[', b'B', ..] => { // Down arrow
                        info!("Down arrow pressed");
                    },
                    [b'[', b'C', ..] => { // Right arrow
                        info!("Right arrow pressed");
                    },
                    [b'[', b'D', ..] => { // Left arrow
                        info!("Left arrow pressed");
                    },
                    [b'[', b'3', b'~', ..] => { // Delete key
                        error!("Decide whether to implement Delete key as a backspace alias or something else (like dropping the top of the stack?)");
                    },
                    [b'\x03', ..] => { // Ctrl-Alt-C
                        // Like Ctrl-C, but with `Some(&mut delay)`
                        error!("Stopped by user (Ctrl-Alt-C). Delaying before reset.");
                        disp_grave_error(&disp_refcell, Some(&mut delay));
                    },
                    [b'[', b'1', b'5', b'~', ..] => { // F5 key
                        // Force a redraw of both textbox and stack
                        // Amongst other effects, this clears the non-grave error icon
                        info!("Doing a forced redraw of both stack and textbox.");
                        
                        // Just to be ultra-sure, we flush both
                        stack.draw(true)
                            .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                            .unwrap();
                        textbox.draw(true)
                            .inspect_err(|e| defmt::panic!("Error with display: {:?}", *e))
                            .unwrap();
                    },
                    _ => {
                        warn!("Unhandled escape sequence received over UART: {:?}", &buf);
                    }
                }

                debug!("Escape sequence received over UART: {:?}", core::str::from_utf8(&buf).unwrap_or("Invalid UTF-8"));
            },

            '\x02' => { // Ctrl-B
                // Here should be a breakpoint for debugging purposes, just to make it easier to break into the debugger while not doing anything
                debug!("Breakpoint!");
            },

            '\x15' => { // Ctrl-U
                info!("Rebooting intto USB bootloader by user request (Ctrl-U)");
                hal::rom_data::reset_to_usb_boot(0, 0); // No pin for activity LED, both MSC and Picoboot enabled – as per cold boot.
            },

            _ => {
                warn!("Unhandled character received over UART: {:?} (0x{:X})", char_buf, buf[0]);
                continue; // Ignore the character
            },
        }
    };
}


/// Display a simple error indication on the display.
/// If `grave` is true, it inverts the display to indicate a grave error and resets,
/// otherwise it only shows a simple error indication.
pub fn disp_grave_error<'a, DI, SIZE>(
    disp_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,
    maybe_delay: Option<&mut cortex_m::delay::Delay>
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

    maybe_delay.expect("No delay provider given, cannot delay before reset. Panicking.")
        .delay_ms(10_000);
    cortex_m::peripheral::SCB::sys_reset(); // Reset the microcontroller
}

pub fn disp_error<'a, DI, SIZE> (
    disp_refcell: &'a RefCell<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>,
) -> ()
where
    DI: WriteOnlyDataCommand,
    SIZE: DisplaySize,
{
    let mut disp = disp_refcell.borrow_mut();

    let bmp = Bmp::from_slice(include_bytes!("calc_err.bmp")).expect("Failed to load error image from memory. Image data must be malformed.");
    let img = Image::new(
        &bmp,
        (117, 0).into(), // Image is 10x10, we put it in the top-right corner
    );
    if let Err(e) = img.draw(disp.deref_mut()) {
        core::panic!("Failed to draw image on display: {:?}", e);
    };
    if let Err(e) = disp.flush() {
        core::panic!("Failed to flush display: {:?}", e);
    };
}

pub const fn text_offset(text_len: u32, display_width: u32, font_width: u32) -> u32 {
    (display_width - (text_len * font_width)) / 2
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
    let txbx_data = textbox.get_text();
    textbox.clear();

    let num = DecimalFixed::parse_static_exp(txbx_data.as_str(), DECFIX_EXPONENT)?;

    stack.push(num)?;

    // We save ourselves a double flush call when drawing both, because I²C ops are slow and blocking
    stack.draw(false)?;
    textbox.draw(flush)?;
    Ok(())
}