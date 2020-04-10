#![no_std]

#[doc(hidden)]
pub unsafe fn any_to_byte_slice<T>(data: &T) -> &[u8] {
    core::slice::from_raw_parts(data as *const _ as *const _, core::mem::size_of::<T>())
}

#[doc(hidden)]
pub unsafe fn get_type_str<T>(_: &T) -> &'static str {
    core::any::type_name::<T>()
}

#[doc(hidden)]
pub union Transmute<T: Copy, U: Copy> {
    pub from: T,
    pub to: U,
}

use core::cell::Cell;

const LOG0_CAPACITY: usize = 1024;

#[no_mangle]
pub static mut LOG0_CURSORS: Cursors = Cursors {
    target: Cell::new(0),
    host: Cell::new(0),
    buf: unsafe { &mut LOG0_BUFFER as *const _ as *mut u8 },
};

#[no_mangle]
static mut LOG0_BUFFER: [u8; LOG0_CAPACITY] = [0; LOG0_CAPACITY];

#[repr(C)]
pub struct Cursors {
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

    pub fn write_frame(&self, sym: *const u8, type_str: *const u8, data: &[u8]) {
        let free = LOG0_CAPACITY
            - 1
            - (self.target.get() - self.host.get() + LOG0_CAPACITY) % LOG0_CAPACITY;
        let len = data.len() + 8;

        if free >= len + 2 {
            for b in &(len as u16).to_le_bytes() {
                self.push(*b);
            }

            for b in &(sym as u32).to_le_bytes() {
                self.push(*b);
            }

            for b in &(type_str as u32).to_le_bytes() {
                self.push(*b);
            }

            for b in data {
                self.push(*b);
            }
        }
    }
}

#[macro_export]
macro_rules! log {
    ($str:literal, $var:ident) => {
        {

            // log0::info!("Look what I got: {}", &TEST1);
            //
            // expands to

            const FMT: &'static str = $str;

            #[link_section = ".fasthosting"]
            static S: [u8; FMT.as_bytes().len()] = unsafe {
                *log0_target::Transmute::<*const [u8; FMT.len()], &[u8; FMT.as_bytes().len()]> {
                    from: FMT.as_ptr() as *const [u8; FMT.as_bytes().len()],
                }
                .to
            };

            let s = unsafe { log0_target::get_type_str(&$var) };
            let v = unsafe { log0_target::any_to_byte_slice(&$var) };


            unsafe {
                log0_target::LOG0_CURSORS.write_frame(&S as *const _, s.as_ptr() as *const _, v);
            }
        }
    };
}

// #[cfg(test)]
// mod tests {
//     #[test]
//     fn it_works() {
//         assert_eq!(2 + 2, 4);
//     }
// }
