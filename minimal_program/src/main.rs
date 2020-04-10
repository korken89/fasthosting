#![no_main]
#![no_std]

pub mod mod1 {
    pub mod mod2 {
        pub struct MyStruct {
            pub b: i32,
            pub d: u32,
        }
    }
}

enum MyEnum {
    Var1,
    Var2((u8, f32)),
    Var3,
}

use cortex_m_rt::entry;
// use cortex_m_semihosting::hprintln;
use panic_halt as _;
use stm32l4xx_hal as _;

static mut TEST1: mod1::mod2::MyStruct = mod1::mod2::MyStruct {
    b: 2,
    d: 4,
};

static mut TEST2: (f32, u32, &str) = (1.0, 2, &"test test");

static mut TEST3: MyEnum = MyEnum::Var2((1, 2.0));

static mut TEST4: f32 = 3.0;

static mut TEST5: () = ();


#[entry]
fn init() -> ! {

    loop {
        unsafe {
            core::ptr::read_volatile(&TEST1);
            core::ptr::read_volatile(&TEST2);
            core::ptr::read_volatile(&TEST3);
            core::ptr::read_volatile(&TEST4);
            core::ptr::read_volatile(&TEST5);
        }
        cortex_m::asm::delay(1_000_000);

        log0_target::log!("Look what I got: {}", TEST1);

        cortex_m::asm::delay(1_000_000);

        log0_target::log!("Look what I got: {}", TEST4);
    }
}
