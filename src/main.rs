//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

// 1 MHz, the maximum speed for I²C on the RP2040 (so-called Fast Mode Plus; datasheet 4.3.3), and the SSD1306 can handle it well
const I2C_FREQ: hal::fugit::HertzU32 = hal::fugit::HertzU32::kHz(1000);

use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use core::cell::RefCell;
use rp2040_hal::{
    self as hal,
    pac,

    clocks::{Clock, init_clocks_and_plls},
    watchdog::Watchdog,
    sio::Sio,
};

// Display imports
use ssd1306::{
    prelude::*,
    Ssd1306,
};

mod stack;
use stack::*;

// ------------------------------------------------------------------------------------------------------------------------------------------------

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

// ------------------------------------------------------------------------------------------------------------------------------------------------

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
    let mut _delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz()); // Unused
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
        I2C_FREQ,
        &mut peri.RESETS,
        &clocks.peripheral_clock,
    );
    trace!("I²C initialized");

    // This helper struct finishes configuring the I2C (most importantly with the display's address)
    // and provides a compatible interface for SSD1306 lib, that itself is generic over the I²C/SPI interface.
    let iface = ssd1306::I2CDisplayInterface::new(i2c);

    let mut disp = Ssd1306::new(iface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode(); // Needed to support embedded-graphics.
    disp.init().unwrap(); // Automatically clears it as well; without that it would show grain as (V)RAM is random on powerup.
    disp.set_brightness(Brightness::BRIGHTEST).unwrap(); // XXX: Good to dim when working at night!
    trace!("Display initialized");

    // ------------------------------------------------------------------------------------------------------------------------------------------------

    info!("Starting the stack jigglery-pokery");

    unsafe { core::arch::asm!("bkpt"); }
    let disp_refcell = RefCell::new(disp);
    // Range of u8 is 0..=255
    let mut stack = CustomStackBuilder::<'_, u8, _, _>::new(&disp_refcell) // We're using the turbofish syntax here
        .build();

    // Push some initial values onto the stack
    //unsafe { core::arch::asm!("bkpt"); }
    stack.push_slice(&[1, 2, 3, 4, 5, 6]).unwrap();
    debug!("Stack: {:?}", stack.peek_all());
    stack.draw();

    // Push another value
    unsafe { core::arch::asm!("bkpt"); }
    stack.push(7).unwrap();
    stack.draw();

    // Peek at the top value
    unsafe { core::arch::asm!("bkpt"); }
    let top = stack.peek().unwrap();
    debug!("Top value is {}", top);
    stack.draw();

    // Pop a value off the stack
    unsafe { core::arch::asm!("bkpt"); }
    let popped = stack.pop().unwrap();
    debug!("Popped value is {}", popped);
    stack.draw();

    // Pop multiple values off the stack
    unsafe { core::arch::asm!("bkpt"); }
    let iter = stack.multipop(3).unwrap();
    debug!("Starting a multipop loop");
    for value in iter {
        unsafe { core::arch::asm!("bkpt"); }
        debug!("Multipopped value: {}", value);

        // We can't get immutable borrow of stack here to debug it,
        // because the iterator still holds a mutable borrow until it's fully consumed.
        // Luckily we can peek at the stack through debugger watches.
        //debug!("Stack now: {:?}", stack);
    }
    unsafe { core::arch::asm!("bkpt"); }
    // The iterator will be fully consumed after the loop ends, releasing the mutable borrow on the stack.
    stack.draw();

    // Peek at top 2 values as a slice
    unsafe { core::arch::asm!("bkpt"); }
    let top_slice = stack.multipeek(2).unwrap();
    debug!("Top 3 values as slice: {:?}", top_slice);
    stack.draw();

    unsafe { core::arch::asm!("bkpt"); }
    if stack.is_empty() {
        error!("Stack is empty!! HOW?!");
    } else {
        debug!("Stack is not empty.");
    }

    unsafe { core::arch::asm!("bkpt"); }
    info!("All done, entering infinite WFI loop");
    loop {
        cortex_m::asm::wfi();
    }
}
