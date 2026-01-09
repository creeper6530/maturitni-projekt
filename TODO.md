- Figure out a way to do UART receiving asynchronously – without polling, but interrupts, DMA or similar funsies. Just so that we don't block and can go to WFI/WFE sleep.
  - I already tried something and failed miserable. That's why we poll ATM.
  - Maybe I should've gone with async Embassy instead...
- Make a common file for all constants instead of them being spread around `stack.rs`, `textbox.rs` and `main.rs`, or at least add runtime checks that matching consts equal.
- Add some functionality to the ANSI escape codes
  - Arrow keys could move the cursor in the textbox – left-right keys; and scroll through either the last inputs (would need history keeping) or through values in stack (would need peeking at arbitrary depth) like in a terminal – up-down keys.
  - Delete key could either operate on the textbox, alias Backspace or drop the topmost item from the stack (alias Shift-D)
  - Maybe use PgUp and PgDn keys to scroll through the stack's preview?
  - F-keys are ANSI escaped and could be used for more advanced functions instead of letter keys
  - See [ANSI escape code#Terminal input sequence](https://en.wikipedia.org/wiki/ANSI_escape_code?useskin=vector#Terminal_input_sequences) for more details
- Consider adding attributes to the `memory.x` linker script, as described [here](https://home.cs.colorado.edu/~main/cs1300/doc/gnu/ld_3.html#SEC37).
- There appears to exist a `rust_decimal` crate, but we're too deep in sunk cost fallacy now...
- *~~Run `cargo fmt` on your code~~* No, `cargo fmt` messes the codebase up too badly and sometimes makes straight up illogical or inconsistent choices, do **NOT** use it. Perhaps we could give it another try with format-on-save when writing some other project anew.

- Perhaps switch from `heapless` to `arrayvec` crate, crate `pio` (dependency of HAL) uses it too at version `v0.7.6`
  - `heapless` is made by the official Embedded WG libs team, but (according to crates.io) is 125 KiB as opposed to `arrayvec`'s 30,5 KiB due to less features overall, though most of it is indeed optimised away anyway
  - If we do take the leap, don't forget to commit and test size with `cargo size` or `cargo bloat --crates`
- Consider using some sort of allocator after all, maybe on SRAM 4 and 5?
  - `talc` seems nice.
  - Would probably need some `static mut`, `link-section` and `memory.x` jigglery-pokery
    - If we just do `Span::from_base_size()`, wouldn't it be easier? Perhaps could even avoid costly initialisation of a static unless `MaybeUninit` helps out.

- Move the library-like files into an actual separate crate that would be taken as a dependency. **TESTS**, documentation, semver, public/private, feature gates and all that jazz.
- Rewrite the swap code and operands (+-*/) to take advantage of the DoubleEndedIterator we return with `stack.multipop()`, though it's possible that it will need some reversing.
- Optimize multiple draws in short succession. Possibly move some draws and flushes after the main match in `main()`?
- Change the useless space-saving attempt of `exponent: i8` that only needs constant converting and is invalidated by alignment -> padding anyway.
  - When deciding, do a search for `exponent as` and `exponent) as` and count the types we're converting into. `u32` appears to lead, but we need `i*`!!
  - Take struct size with padding into question. It appears to be pegged at size=16 align=0x8 up until `i64`.
- All in all get rid of the wonky situation with typing in draw()-s

# START STABILIZING AND DOCUMENTING!!!
- Update obsolete comments (needs thorough review)