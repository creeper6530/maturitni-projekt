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
use embedded_graphics::{prelude::*, image::Image, pixelcolor::BinaryColor};
use ssd1306::{prelude::*, Ssd1306};
use tinybmp::Bmp;

use core::cell::RefCell;
//use core::ops::DerefMut;

mod stack;
use stack::*;
mod textbox;
use textbox::*;
mod decfix;
use decfix::DecimalFixed;

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
    let mut peri = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
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
    ).unwrap();
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
    disp.init().unwrap();
    disp.set_brightness(Brightness::BRIGHTEST).unwrap();

    // We show a Rust logo bitmap on the display as a loading screen
    // We're showing it as soon as possible once the display and everything it needs is initialized
    Image::new(
        &Bmp::from_slice(include_bytes!("rust.bmp")).unwrap(),
        (32, 0).into(), // The image is 64x64, so we center it horizontally
    )
    .draw(&mut disp).unwrap();
    disp.flush().unwrap();
    trace!("Display initialized");

    // Let me ask one question: Why the hell can't this be as straightforward as I²C is?
    let uart = hal::uart::UartPeripheral::new(
        peri.UART0,
        (pins.gpio0.into_function(), pins.gpio1.into_function()), // Luckily the function itself is inferred, so we don't need to specify it explicitly
        &mut peri.RESETS
    )
    .enable(hal::uart::UartConfig::default(), clocks.peripheral_clock.freq()) // Default is a sane 115200 8N1
    .unwrap();
    let (rx, tx) = uart.split();
    trace!("UART initialized");
    
    // Here we're basically just flexing that we can use DMA :D
    let dma = peri.DMA.split(&mut peri.RESETS);
    let tx_transfer = DmaSingleBufferConfig::new(dma.ch0, b"\x1b[2J\x1b[HUART initialised!\r\n", tx).start(); // Send a message over UART using DMA, also clear the terminal (VT100 codes)

    // ----------------------------------------------------------------------------

    let disp_refcell = RefCell::new(disp);
    // Range of i32 is `-2147483648..=2147483647`
    let mut stack: CustomStack<'_, DecimalFixed, _, _> = CustomStackBuilder::<'_, DecimalFixed, _, _>::new(&disp_refcell) // We're using the turbofish syntax here
        .build();
    let mut textbox: _ = CustomTextboxBuilder::new(&disp_refcell)
        .build();

    //stack.push_slice(&[5, 6, 7, 8, 9, 10]).unwrap();
    //textbox.append_str("DEBUG TEXTBOX DEBUG!").unwrap();

    trace!("Stack and textbox initialized, showing off the logo for a bit");

    delay.delay_ms(200); // Just to show the Rust logo for a bit
    stack.draw(false);
    textbox.draw(true);

    trace!("Waiting for initial DMA transfer to complete, should be instant");
    let (ch0, _, tx) = tx_transfer.wait(); // So that we can reuse them. We don't really care about reclaiming the &'static buffer tho, so we ignore it
    trace!("Finished waiting");
    let _new_tx_transfer = DmaSingleBufferConfig::new(ch0, b"Entering main loop\r\n", tx).start(); // Send another message with DMA, this time we don't need to reclaim the channel, so we don't wait for it to finish
    info!("Entering main loop");

    loop {
        let mut buf: [u8; 1] = [0];
        rx.read_full_blocking(&mut buf).unwrap(); // TODO: Figure out a way to do this non-blocking, perhaps with DMA and/or an interrupt. I tried and failed miserably.

        let char_buf = char::from_u32(buf[0] as u32).unwrap_or('?'); // Replace invalid UTF-8 with a replacement character

        match char_buf {
            '\r' | '\n' => { // Enter or newline
                if textbox.len() == 0 {
                    continue; // Ignore empty textbox
                }

                let txbx_data = textbox.get_text();
                textbox.clear();

                // XXX: If you change the stack type, you need to change this too
                //match txbx_data.as_str().parse::<DecimalFixed>() {
                match DecimalFixed::parse_static_exp(txbx_data.as_str(), -9) {
                    Ok(num) => {
                        match stack.push(num) {
                            Ok(_) => {
                                // We save ourselves a double flush call when drawing both, because I²C ops are slow and blocking
                                stack.draw(false);
                                textbox.draw(true);
                            },
                            Err(e) => {
                                error!("Failed to push number onto stack: {:?}", e);
                                textbox.append_str("Err").unwrap(); // HACK: Show error on display in a better way than contaminating the textbox
                                textbox.draw(true);
                            },
                        };
                    },
                    Err(_) => {
                        error!("Failed to parse input as number (ParseIntError)");
                        warn!("This should normally be impossible, textbox must be contaminated");
                        textbox.append_str("Err").unwrap(); // HACK: Show error on display in a better way than contaminating the textbox... again
                        textbox.draw(true);
                    },
                };
            },

            '\u{8}' | '\u{7F}' => { // Backspace or Delete
                #[cfg(debug_assertions)]
                debug!("Backspace character received: (0x{:X})", buf[0]);

                if textbox.len() == 0 {
                    info!("Ignoring backspace on empty textbox");
                    continue; // Ignore backspace on empty textbox
                }
                match textbox.backspace(1) {
                    Ok(_) => {},
                    Err(_) => {
                        error!("Failed to backspace textbox");
                        textbox.append_str("Err").unwrap(); // HACK
                        textbox.draw(true);
                        continue;
                    },
                };
                textbox.draw(true);
                continue;
            },

            '.' | ',' => { // Decimal point
                if textbox.len() == 0 {
                    textbox.append_str("0.").unwrap();
                    textbox.draw(true);
                    continue;
                }
                if textbox.contains('.') {
                    warn!("Ignoring decimal point, textbox already contains one");
                    continue; // Ignore decimal point if there's already one in the textbox
                }
                textbox.append_char('.').unwrap();
                textbox.draw(true);
                continue;
            },

            '0'..='9' => { // Digits
                textbox.append_char(char_buf).unwrap();
                textbox.draw(true);
                continue;
            },

            '+' | '-' | '*' | '/' => {
                if textbox.len() != 0 {
                    let txbx_data = textbox.get_text();
                    textbox.clear();

                    //let num_res = data.parse::<DecimalFixed>(); // XXX: If you change the stack type, you need to change this too
                    let num_res = DecimalFixed::parse_static_exp(txbx_data.as_str(), -9);
                    match num_res {
                        Ok(num) => {
                            match stack.push(num) {
                                Ok(_) => {}, // Do nothing
                                Err(e) => {
                                    error!("Failed to push number onto stack: {:?}", e);
                                    textbox.append_str("Err").unwrap(); // HACK
                                    textbox.draw(true);
                                    continue; // There's something beyond this if-statement, so we need to avoid executing it because we encountered an error
                                },
                            };
                        },
                        Err(_e) => {
                            error!("Failed to parse input as number (ParseIntError)");
                            warn!("This should normally be impossible, textbox must be contaminated");
                            textbox.append_str("Err").unwrap(); // HACK
                            textbox.draw(true);
                            continue;
                        },
                    };
                }

                // Since the stack is LIFO, the A is the one pushed earlier, so it is popped later
                // So that for "5 6 -", we do 5 - 6, not 6 - 5
                let b_res = stack.pop();
                let a_res = stack.pop();

                let c = match (a_res, b_res) {
                    (Some(a), Some(b)) => {

                        match char_buf {
                            '+' => a + b,
                            '-' => a - b,
                            '*' => a.priv_mul(b, true).unwrap(), // HACK: Don't unwrap()
                            '/' => { // FIXME: Division seems to be broken when dividing integers that don't divide evenly.
                                     /* For example 5 / 2 = 2.5, but we get 2; 
                                     However, 20.5 / 2 = 10.25, but we get 10.2 , so the exponent seems to be handled correctly, just there's not enough of it.
                                     When I try 20.50 / 2 I get 10.25, or 20.500 / 2 = 10.250.
                                     Either automatically increasing the exponent when needed, or doing a reasonable default exponent for parse() would be nice.
                                     (Currently parse() gives only as much exponent as it needs to represent the number)
                                     
                                     Also interesting is that 5.0 / 2 gives correct 2.5, but 5.0 / 2.0 gives 2 */
                                if b.is_zero() {
                                    error!("Division by zero");
                                    textbox.append_str("Err").unwrap(); // HACK
                                    textbox.draw(true);
                                    continue;
                                } else {
                                    a.priv_div(b, true).unwrap() // HACK: Don't unwrap()
                                }
                            },
                            _ => defmt::unreachable!(), // We already checked this above
                        }

                    },
                    (None, Some(_)) | (None, None) => {
                        error!("Failed to pop number from stack");
                        textbox.append_str("Err").unwrap(); // HACK
                        stack.draw(false); // Redraw stack because we popped something
                        textbox.draw(true);
                        continue;
                    },
                    (Some(_), None) => defmt::unreachable!(), // This should be impossible. How can we first fail but then succeed?
                };

                match stack.push(c) {
                    Ok(_) => {
                        stack.draw(false);
                        textbox.draw(true);
                    },
                    Err(_) => {
                        error!("Failed to push result onto stack");
                        textbox.append_str("Err").unwrap(); // HACK
                        textbox.draw(true);
                        continue;
                    },
                };
            },

            'c' => { // Clear textbox
                textbox.clear();
                textbox.draw(true);
                continue;
            },

            'C' => { // Clear everything (we assume the Shift-C is enough of a modifier)
                textbox.clear();
                stack.clear();
                textbox.draw(false);
                stack.draw(true);
                continue;
            },

            'd' => { // Duplicate the top element of the stack
                match stack.pop() {
                    Some(val) => {
                        for _ in 0..2 { // We pop once, so we need to push twice to duplicate
                            match stack.push(val) {
                                Ok(_) => {},
                                Err(e) => {
                                    error!("Failed to push number onto stack: {:?}", e);
                                    textbox.append_str("Err").unwrap(); // HACK
                                    textbox.draw(true);
                                    continue;
                                },
                            };
                        }
                        stack.draw(true);
                        continue;
                    },
                    None => {
                        error!("Failed to duplicate top element of stack: stack is empty");
                        textbox.append_str("Err").unwrap(); // HACK
                        textbox.draw(true);
                        continue;
                    },
                };
            },

            'D' => { // Drop the topmost element of the stack
                match stack.pop() {
                    Some(_) => {
                        stack.draw(true);
                        continue;
                    },
                    None => {
                        info!("Failed to drop top element of stack: stack is empty. Not an error, only ignoring.");
                        continue;
                    },
                };
            },

            's' => { // Swap the top two elements of the stack
                // B was pushed later, so it is popped first
                let option_b = stack.pop();
                let option_a = stack.pop();

                match (option_a, option_b) {
                    (Some(a), Some(b)) => {
                        // Earlier B was pushed later, so it is now pushed irst
                        match stack.push(b) {
                            Ok(_) => {},
                            Err(e) => {
                                error!("Failed to push number onto stack: {:?}", e);
                                textbox.append_str("Err").unwrap(); // HACK
                                textbox.draw(true);
                                continue;
                            },
                        };
                        match stack.push(a) {
                            Ok(_) => {},
                            Err(e) => {
                                error!("Failed to push number onto stack: {:?}", e);
                                textbox.append_str("Err").unwrap(); // HACK
                                textbox.draw(true);
                                continue;
                            },
                        };
                        stack.draw(true);
                        continue;
                    },
                    (None, Some(a)) => {
                        info!("Failed to swap top two elements of stack: stack has only one element. Not an error, only ignoring.");
                        match stack.push(a) {
                            Ok(_) => {},
                            Err(e) => {
                                error!("Failed to push number onto stack: {:?}", e);
                                textbox.append_str("Err").unwrap(); // HACK
                                textbox.draw(true);
                                continue;
                            },
                        };
                    },
                    (None, None) => {
                        info!("Failed to swap top two elements of stack: stack is empty. Not an error, only ignoring.");
                        continue;
                    },
                    (Some(_), None) => defmt::unreachable!(),
                };
            },

            '\x03' => { // Ctrl-C
                // XXX: Add potential explicit cleanup code here
                drop(stack);
                drop(textbox);
                {
                    trace!("Consuming the refcell and deinitializing the display");
                    let mut disp = disp_refcell.into_inner();
                    disp.clear(BinaryColor::Off).unwrap();
                    disp.flush().unwrap();
                    disp.set_display_on(false).unwrap();
                    disp.release() // Release the I²C interface
                    .release() // Release the I²C peripheral
                    .free(&mut peri.RESETS); // Free the I²C peripheral
                }

                defmt::panic!("Stopped by user (Ctrl-C)");
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
                    _ => {
                        warn!("Unhandled escape sequence received over UART: {:?}", &buf);
                        continue; // Ignore the sequence
                    }
                }

                debug!("Escape sequence received over UART: {:?}", core::str::from_utf8(&buf).unwrap_or("Invalid UTF-8"));
            },

            'B' => {
                // Here should be a breakpoint for debugging purposes, just to make it easier to break into the debugger
                debug!("Breakpoint!");
            },

            _ => {
                warn!("Unhandled character received over UART: {:?} (0x{:X})", char_buf, buf[0]);
                continue; // Ignore the character
            },
        }
    };
}