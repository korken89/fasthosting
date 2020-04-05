target extended-remote :3333

set print asm-demangle on

monitor arm semihosting enable
monitor reset halt

load

# start the process but immediately halt the processor
# start
monitor reset halt


# *try* to stop at the user entry point (it might be gone due to inlining)
# break main

# detect unhandled exceptions, hard faults and panics
# break HardFault
# break core::panicking::panic_fmt
# break rust_begin_unwind
# break core::result::Result<T,E>::unwrap

compare-sections
