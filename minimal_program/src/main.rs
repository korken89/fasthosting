#![no_main]
#![no_std]

struct MyStruct {
    a: f32,
    b: i32,
    c: u8,
    d: u32,
    e: &'static str,
}

use cortex_m_rt::entry;
use panic_halt as _;
use stm32l4xx_hal as _;

static mut TEST: MyStruct = MyStruct {
    a: 1.0,
    b: 2,
    c: 3,
    d: 4,
    e: &"test test",
};

#[entry]
fn init() -> ! {
    loop {
        unsafe {
            core::ptr::read_volatile(&TEST);
        }

        continue;
    }
}
