# README
This project works as a RPN calculator running on the RP2040 microcontroller. Work in progress.

You can render this file at https://markdownlivepreview.com/ if you don't already have a way to view it.

## Hardware:
- Raspberry Pi Pico (recommended in H variant)
  - Possible to use another RP2040-based board
- Raspberry Pi Debug Probe
  - You can use a second Pico in its place, see [here](https://www.raspberrypi.com/documentation/microcontrollers/pico-series.html#debugging-using-another-pico-series-device)
- SSD1306-based OLED display
  - Monochromatic
  - 128x64 px
  - Capable of I²C interfacing
- Some jumper wires
- Breadboard (recommended)

Connect display as follows (recommended pin in bold):
- VCC --> pin 36 (3V3 Out)
- GND --> pin 3, 8, 13, 18, 23, 28 or **38** (GND)
- SDA --> pin 11 (GP8 - SDA)
- SCL --> pin 12 (GP9 - SCL)

Connect Debug Probe's SWD interface to the debug header, and its UART interface as follows:
- RX --> pin 1 (GP0 - TX)
- TX --> pin 2 (GP1 - RX)
- GND --> pin **3**, 8, 13, 18, 23, 28 or 38 (GND)

## Compilation
1. Install the Rust compiler on your platform: https://rust-lang.org/tools/install/
2. Install the toolchain:
    ```
    rustup target install thumbv6m-none-eabi
    ```
3. Clone this repository and switch to appropriate branch
4. Install the linker:
    ```
    cargo install flip-link
    ```
5. Install the flasher:
    ```
    cargo install --locked probe-rs-tools
    ```
6. Power on your Pico and connect your Debug Probe
7. Compile and flash the project:
    ```
    cargo run
    ```