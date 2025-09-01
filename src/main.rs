//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

const I2C_FREQ_KHZ: u32 = 1000; // 1 MHz, the maximum speed for I²C on the RP2040 (so-called Fast Mode Plus; datasheet 4.3.3), and the SSD1306 can handle it well

use defmt::*;
use defmt_rtt as _;
use panic_probe as _;
// TODO: Use other channels of RTT by using `rtt-target` crate instead of `defmt-rtt`
// https://docs.rs/rtt-target/latest/rtt_target/#defmt-integration
// Perhaps even use this instead of UART for terminal?

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
use cortex_m::asm;

// Display imports
use embedded_graphics::{prelude::*, image::Image};
use ssd1306::{prelude::*, Ssd1306};
use tinybmp::Bmp;

use core::cell::RefCell;
//use core::ops::DerefMut;

mod stack;
use stack::*;
mod textbox;
use textbox::*;

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
    disp.set_brightness(Brightness::BRIGHTEST).unwrap(); // TODO: Set the brightness with a potentiometer (probably poll, not ADC interrupt due to noise)

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
    
    let dma = peri.DMA.split(&mut peri.RESETS);
    let tx_transfer = DmaSingleBufferConfig::new(dma.ch0, b"UART initialised!\r\n", tx).start(); // Send a message over UART using DMA
    let (_ch0, _, _tx) = tx_transfer.wait(); // So that we can reuse them

    // ----------------------------------------------------------------------------

    let disp_refcell = RefCell::new(disp);
    // Range of isize is `-2147483648..=2147483647`
    let mut stack: CustomStack<'_, isize, _, _> = CustomStackBuilder::<'_, isize, _, _>::new(&disp_refcell) // We're using the turbofish syntax here
        .build();
    let mut textbox: _ = CustomTextboxBuilder::new(&disp_refcell)
        .build();

    stack.push_slice(&[5, 6, 7, 8, 9, 10]).unwrap();
    //textbox.append_str("DEBUG TEXTBOX DEBUG!").unwrap();

    delay.delay_ms(2_000);
    stack.draw(false);
    textbox.draw(true);

    info!("Entering main loop");

    loop {
        let mut buf: [u8; 1] = [0];
        rx.read_full_blocking(&mut buf).unwrap(); // TODO: Figure out a way to do this non-blocking, perhaps with DMA and an interrupt. I tried and failed miserably.

        let char_buf = char::from_u32(buf[0] as u32).unwrap_or('\u{FFFD}'); // Replace invalid UTF-8 with the replacement character

        match char_buf {
            '\r' | '\n' => { // Enter or newline
                textbox.clear();

                // TODO: Process the input and push something to the stack
            },
            '\u{8}' | '\u{7F}' => { // Backspace or Delete
                textbox.backspace(1).unwrap();
            },
            '0'..='9' => { // Digits
                textbox.append_char(char_buf).unwrap();
            },
            _ => {
                warn!("Unhandled character received over UART: {:?}", char_buf);
                continue; // Ignore the character
            },
        }

        textbox.draw(true);
    };
}