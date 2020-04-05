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

const LOG0_CAPACITY: usize = 1024;

use core::cell::Cell;

#[repr(C)]
struct Cursors {
    target: Cell<usize>,
    host: Cell<usize>,
    buf: *mut u8,
}

impl Cursors {
    fn push(&self, byte: u8) {
        let target = self.target.get();
        unsafe { self.buf.add(target).write(byte) }
        self.target.set(target.wrapping_add(1) % LOG0_CAPACITY);
    }

    fn write_frame(&self, sym: *const u8, type_str: *const u8, data: &[u8]) {
        let free = LOG0_CAPACITY
            - 1
            - (self.target.get() - self.host.get() + LOG0_CAPACITY) % LOG0_CAPACITY;
        let len = data.len() + 8;

        if free >= len {
            for b in &(sym as u32).to_ne_bytes() {
                self.push(*b);
            }

            for b in &(type_str as u32).to_ne_bytes() {
                self.push(*b);
            }

            for b in data {
                self.push(*b);
            }
        }
    }
}

#[no_mangle]
static mut LOG0_CURSORS: Cursors = Cursors {
    target: Cell::new(0),
    host: Cell::new(0),
    buf: unsafe { &mut LOG0_BUFFER as *const _ as *mut u8 },
};

#[no_mangle]
static mut LOG0_BUFFER: [u8; LOG0_CAPACITY] = [0; LOG0_CAPACITY];

#[entry]
fn init() -> ! {
    // TODO:
    //
    // log0::info!("Look what I got: {}", &TEST1);
    //
    // expands to

    const FMT: &'static str = "Look what I got: {}";

    #[link_section = ".crapsection"]
    static S: [u8; FMT.as_bytes().len()] = unsafe {
        *Transmute::<*const [u8; FMT.len()], &[u8; FMT.as_bytes().len()]> {
            from: FMT.as_ptr() as *const [u8; FMT.as_bytes().len()],
        }
        .to
    };

    let s = unsafe { get_type_str(&TEST1) };
    let v = unsafe { any_to_byte_slice(&TEST1) };

    // hprintln!(
    //     "Dump - sym: {:#010x}, type_str: {:#010x}, len: {}, str: {}, data: {:?}",
    //     &S as *const _ as usize,
    //     s.as_ptr() as *const _ as usize,
    //     s.len(),
    //     s,
    //     v
    // )
    // .ok();

    loop {
        unsafe {
            core::ptr::read_volatile(&TEST1);
            core::ptr::read_volatile(&TEST2);
            core::ptr::read_volatile(&TEST3);
            core::ptr::read_volatile(&TEST4);
            core::ptr::read_volatile(&TEST5);
            core::ptr::read_volatile(&LOG0_CURSORS);
        }
        cortex_m::asm::delay(1_000_000);

        unsafe {
            LOG0_CURSORS.write_frame(&S as *const _, s.as_ptr() as *const _, v);
            // let val = LOG0_CURSORS.target.get();
            // LOG0_CURSORS.target.set(val + 1);
        }
    }
}
