#![no_std]
#![no_main]

use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use rp2040_hal::{
    self as hal,
    pac,

    clocks::{Clock, init_clocks_and_plls},
    watchdog::Watchdog,
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

    // ------------------------------------------------------------------------------------------------------------------------------------------------

    info!("Starting the stack jigglery-pokery");

    unsafe { core::arch::asm!("bkpt"); }
    // Range of u8 is 0..=255
    let mut stack = CustomStack::<u8>::new(); // We're using the turbofish syntax here

    // Push some initial values onto the stack
    unsafe { core::arch::asm!("bkpt"); }
    //stack.push_slice(&[1, 2, 3, 4, 5, 6]).unwrap();
    stack.push_array([4, 5, 6]).unwrap();
    stack.debug_print();

    // Push another value
    unsafe { core::arch::asm!("bkpt"); }
    stack.push(7).unwrap();
    stack.debug_print();

    // Peek at the top value
    unsafe { core::arch::asm!("bkpt"); }
    let top = stack.peek().unwrap();
    debug!("Top value is {}", top);
    stack.debug_print();

    // Pop a value off the stack
    unsafe { core::arch::asm!("bkpt"); }
    let popped = stack.pop().unwrap();
    debug!("Popped value is {}", popped);
    stack.debug_print();

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

        //stack.debug_print(); // error[E0502]: cannot borrow `stack` as immutable because it is also borrowed as mutable
    }
    unsafe { core::arch::asm!("bkpt"); }
    // The iterator will be fully consumed after the loop ends, releasing the mutable borrow on the stack.
    stack.debug_print();

    unsafe { core::arch::asm!("bkpt"); }
    if stack.is_empty() {
        defmt::panic!("Stack is empty!! HOW?!");
    } else {
        debug!("Stack is not empty.");
    }

    // Peek at top 2 values as a slice
    unsafe { core::arch::asm!("bkpt"); }
    let top_slice = stack.multipeek(2).unwrap();
    debug!("Top 3 values as slice: {:?}", top_slice);
    stack.debug_print();

    unsafe { core::arch::asm!("bkpt"); }
    info!("All done, entering infinite WFI loop");
    loop {
        cortex_m::asm::wfi();
    }
}