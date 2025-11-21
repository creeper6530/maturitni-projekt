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

use rp2040_hal as hal;
use hal::{
    pac,
    sio::Sio,

    clocks::{Clock, init_clocks_and_plls},
    watchdog::Watchdog,
};
use cortex_m::asm;

// Display imports
use ssd1306::{prelude::*, Ssd1306};
use embedded_graphics::{
    prelude::*,
    pixelcolor::BinaryColor,

    mono_font::{
//        ascii::FONT_6X12,
        iso_8859_2::FONT_6X12 as ISO_FONT_6X12,
        MonoTextStyle
    },
    text::{
        Baseline,
        Alignment,
        TextStyleBuilder,

        Text,
    },

    primitives::{
        PrimitiveStyleBuilder,
        StrokeAlignment,

        Rectangle,
        Triangle,
    },

    image::Image,
};
use tinybmp::Bmp;

#[unsafe(link_section = ".boot2")]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

defmt::timestamp!("{=u64:us}", {
    /* Stolen from `https://docs.rs/rp2040-hal/latest/src/rp2040_hal/timer.rs.html#69-88`
    and `https://defmt.ferrous-systems.com/timestamps`, though customised greatly.
    We use the critical section to ensure no disruptions, because reading L latches the H register (datasheet section 4.6.2)
    It could have unforseen consequences if we try reading again while there's already a read in progress. */

    // Safety: We are guaranteed that the PTR points to a valid place, since we assume the `pac` is infallible.
    let timer_regs = unsafe { &*pac::TIMER::PTR }; // We dereference the TIMER peripheral's raw pointer and get a normal reference to it. And no, we can't do this in a const nor a static.
    critical_section::with(|_| {
        let low: u32 = timer_regs.timelr().read().bits();
        let hi: u32 = timer_regs.timehr().read().bits();
        ((hi as u64) << 32) | (low as u64)
    })
});

#[hal::entry]
fn main() -> ! {
    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

    let clocks = init_clocks_and_plls(
        12_000_000u32,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    ).unwrap();
    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());
    debug!("Clocks initialized");

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let i2c = hal::I2C::i2c0(
        pac.I2C0,
        pins.gpio8.reconfigure(),
        pins.gpio9.reconfigure(),
        I2C_FREQ,
        &mut pac.RESETS,
        &clocks.peripheral_clock,
    );
    debug!("I2C basics initialized");

    // This helper struct finishes configuring the I2C (most importantly with the display's address) and provides a compatible interface for SSD1306 lib.
    let iface = ssd1306::I2CDisplayInterface::new(i2c);

    let mut disp = Ssd1306::new(iface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode(); // Needed to support embedded-graphics.
    disp.init().unwrap(); // Automatically clears it as well; without that it would show grain as (V)RAM is random on powerup.
    disp.set_brightness(Brightness::BRIGHTEST).unwrap(); // XXX: Good to dim when working at night!
    info!("Display initialized");

    // We show a Rust logo bitmap on the display just to show off images.
    // Look at commit 59f55b280c9fee0391c036f87171be7993ee8497 to see more about images.
    // You can make compatible 1-bit BMPs at https://convertico.com/png-to-bmp/
    Image::new(
        &Bmp::from_slice(include_bytes!("rust.bmp")).unwrap(), // The include_bytes! macro yields a `&'static [u8; N]` slice equal to the file bytes.
        (32, 0).into(), // The image is 64x64, so we center it horizontally, since the position is top-left corner.
    ).draw(&mut disp).unwrap();

    // Since every debug goes with a timestamp, this measures the time taken to flush it
    trace!("Flushing");
    disp.flush().unwrap(); // The draw method only draws to a buffer (for performance), we need to flush it over I2C.
    trace!("Flushed");

    info!("Showed an image, delaying...");
    delay.delay_ms(3_000);

    debug!("Drawing all the funsies");

    disp.clear_buffer(); // We don't want to draw over the image

    // Standard white text on transparent background using supplied font that supports Czech
    let character_style = MonoTextStyle::new(&ISO_FONT_6X12, BinaryColor::On);

    /* Yes, I could just use the default or do with_baseline, but I want to demonstrate both alignment and baseline options.

    The baseline: I'm used to specifying top-left corner instead of some shitty "baseline" where glyphs hang below, like 'p' or 'y'.
    Do I look to you like a fucking typographer? I'm sleep deprived and just want a predictable position without overlaps!
    (Well, technically here I'm specifying the top-middle because of center alignment, but still, it's top.)

    The alingment: We're aligning to center just to show off. Too impractical in the actual project.
    But hey, when I want to center something next time, now I know that I can just specify WIDTH/2
    and let the library handle it, not having to write custom macro to determine the coords. */
    let text_style = TextStyleBuilder::new()
        .baseline(Baseline::Top)
        .alignment(Alignment::Center)
        .build();

    // Standard white stroke with 2px width and transparent fill
    let primitives_style = PrimitiveStyleBuilder::new()
        .stroke_color(BinaryColor::On)
        .stroke_width(2)
        .stroke_alignment(StrokeAlignment::Inside)
        .build();

    // Draw a rectangle over the entire screen
    Rectangle::new(
        (0, 0).into(), // Top-left corner
        (128, 64).into() // Size (here equals size of display)
    ).into_styled(primitives_style) // Style it with the appropriate style
    .draw(&mut disp)
    .unwrap();

    // Another, smaller square
    Rectangle::new(
        (5, 5).into(), // Top-left corner
        (35, 35).into() // Size (here equals size of display)
    ).into_styled(primitives_style) // Style it with the appropriate style
    .draw(&mut disp)
    .unwrap();

    // Randomly chosen points for the triangle
    Triangle::new(
        (20, 20).into(),
        (105, 15).into(),
        (70, 45).into()
    ).into_styled(primitives_style)
    .draw(&mut disp)
    .unwrap();

    Text::with_text_style(
        "Příliš žluťoučký",
        ((128 / 2), (64 - 15)).into(), // Position: with the used baseline and alignment, this is top-center
        character_style, // Text doesn't do into_styled because there are two "styles"
        text_style,
    ).draw(&mut disp)
    .unwrap();

    trace!("Flushing");
    disp.flush().unwrap();
    trace!("Flushed");
    info!("Display drawn to and flushed. Goodnight.");

    loop {
        asm::wfi(); // Just repeatedly go into sleep mode
    }
}