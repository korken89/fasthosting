#![no_main]
#![no_std]

pub mod mod1 {
    pub mod mod2 {
        pub struct MyStruct {
            pub a: f32,
            pub b: i32,
            pub c: u8,
            pub d: u32,
            pub e: &'static str,
        }
    }
}

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use panic_halt as _;
use stm32l4xx_hal as _;

static mut TEST: mod1::mod2::MyStruct = mod1::mod2::MyStruct {
    a: 1.0,
    b: 2,
    c: 3,
    d: 4,
    e: &"test test",
};

unsafe fn any_to_byte_slice<T>(data: &T) -> &[u8] {
    core::slice::from_raw_parts(data as *const _ as *const _, core::mem::size_of::<T>())
}

unsafe fn get_type_str<T>(_: &T) -> &'static str {
    core::any::type_name::<T>()
}

// const fn test() -> [u8; 10] {
//     const STR: &str = "asdfxcvbrt";
//
//     union Transmute<T: Copy, U: Copy> {
//         from: T,
//         to: U,
//     }
//
//     unsafe {
//         *Transmute::<*const [u8; STR.len()], &[u8; STR.len()]> {
//             from: STR.as_ptr() as *const [u8; STR.len()],
//         }
//         .to
//     }
// }

union Transmute<T: Copy, U: Copy> {
    from: T,
    to: U,
}

// const TN: &'static str = "my string that can be a type";
//
// static S: [u8; TN.as_bytes().len()] = unsafe {
//     *Transmute::<*const [u8; TN.len()], &[u8; TN.as_bytes().len()]> {
//         from: TN.as_ptr() as *const [u8; TN.as_bytes().len()],
//     }
//     .to
// };

#[entry]
fn init() -> ! {
    // TODO:
    //
    // log0::info!("Look what I got: {}", &TEST);
    //
    // expands to
    {
        const FMT: &'static str = "Look what I got: {}";

        #[link_section = ".crapsection"]
        static S: [u8; FMT.as_bytes().len()] = unsafe {
            *Transmute::<*const [u8; FMT.len()], &[u8; FMT.as_bytes().len()]> {
                from: FMT.as_ptr() as *const [u8; FMT.as_bytes().len()],
            }
            .to
        };

        let s = unsafe { get_type_str(&TEST) };
        let v = unsafe { any_to_byte_slice(&TEST) };

        hprintln!(
            "Dump - sym: {:#010x}, type_str: {:#010x}, len: {}, str: {}, data: {:?}",
            &S as *const _ as usize,
            s.as_ptr() as *const _ as usize,
            s.len(),
            s,
            v
        )
        .ok();
    }

    loop {
        unsafe {
            core::ptr::read_volatile(&TEST);
        }
    }
}
