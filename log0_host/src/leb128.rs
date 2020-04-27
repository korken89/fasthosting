pub(crate) const CONTINUE: u8 = 1 << 7;

/// Try to decode a LEB128 encoded u32, returns `(value, bytes used)` if successful
pub fn decode_u32<'a, T: Iterator<Item = &'a u8>>(bytes: T) -> Result<(u32, usize), ()> {
    let mut val = 0;

    for (i, byte) in bytes.enumerate() {
        val |= u32::from(*byte & !CONTINUE) << (7 * i);

        if *byte & CONTINUE == 0 {
            return Ok((val, i + 1));
        }
    }

    Err(())
}
