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
    /// NB: Assumes there is space in the buffer for the data
    fn push(&self, byte: u8) {
        let target = self.target.get();
        unsafe { self.buf.add(target).write(byte) }
        self.target.set(target.wrapping_add(1) % LOG0_CAPACITY);
    }

    /// NB: Assumes there is space in the buffer for the data
    fn leb128_write(&self, mut word: u32) {
        const CONTINUE: u8 = 1 << 7;

        loop {
            let mut byte = (word & 0x7f) as u8;
            word >>= 7;

            if word != 0 {
                byte |= CONTINUE;
            }
            self.push(byte);

            if word == 0 {
                return;
            }
        }
    }

    fn len(&self) -> usize {
        self.target
            .get()
            .wrapping_sub(self.host.get())
            .wrapping_add(LOG0_CAPACITY)
            % LOG0_CAPACITY
    }

    fn free(&self) -> usize {
        LOG0_CAPACITY - 1 - self.len()
    }

    #[doc(hidden)]
    pub fn write_frame(&self, sym: *const u8, type_str: *const u8, data: &[u8]) {
        let data_len = data.len();

        // Worst case, data length + 3 LEB encoded u32s, never really happens
        if self.free() >= data_len + 15 {
            self.leb128_write(data_len as u32);
            self.leb128_write(sym as u32);
            self.leb128_write(type_str as u32);

            // TODO: Replace with a copy of the buffer + single update of the target cursor
            for b in data {
                self.push(*b);
            }
        }
    }
}

#[macro_export]
macro_rules! log {
    ($str:literal, $var:ident) => {{
        // log0::info!("Look what I got: {}", &TEST1);
        //
        // expands to
        //
        // TODO: Move to proc macro to do the string checking

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
    }};
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[macro_use]
extern crate std;
