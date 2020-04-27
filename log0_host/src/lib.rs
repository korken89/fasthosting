
use std::collections::VecDeque;

#[cfg(test)]
mod tests;

pub struct Packet {
    pub string_loc: usize,
    pub type_loc: usize,
    pub buffer: Vec<u8>,
}

const CONTINUE: u8 = 1 << 7;

fn leb128_decode_u32<'a, T: Iterator<Item = &'a u8>>(bytes: T) -> Result<(u32, usize), ()> {
    let mut val = 0;
    for (i, byte) in bytes.enumerate() {
        val |= u32::from(*byte & !CONTINUE) << (7 * i);

        if *byte & CONTINUE == 0 {
            return Ok((val, i + 1));
        }
    }

    Err(())
}

fn try_leb128(q: &mut VecDeque<u8>) -> Option<u32> {
    let slices = q.as_slices();

    if let Ok((val, len_used)) = leb128_decode_u32(slices.0.iter().chain(slices.1.iter())) {
        for _ in 0..len_used {
            q.pop_front();
        }

        Some(val)
    } else {
        None
    }
}

pub struct Parser {
    buf: VecDeque<u8>,
    data_size: Option<usize>,
    sym: Option<u32>,
    typ: Option<u32>,
}

impl Parser {
    pub fn new() -> Self {
        Parser {
            buf: VecDeque::with_capacity(10 * 1024 * 1024),
            data_size: None,
            sym: None,
            typ: None,
        }
    }

    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend(data.iter());
    }

    pub fn try_parse(&mut self) -> Option<Packet> {
        loop {
            match (self.data_size, self.sym, self.typ) {
                (None, None, None) => {
                    self.data_size = Some(try_leb128(&mut self.buf)? as usize);
                }
                (Some(_), None, None) => {
                    self.sym = Some(try_leb128(&mut self.buf)?);
                }
                (Some(_), Some(_), None) => {
                    self.typ = Some(try_leb128(&mut self.buf)?);
                }
                (Some(data_size), Some(sym), Some(typ)) => {
                    // Wait for the data payload
                    if self.buf.len() >= data_size {
                        let buf = self.buf.drain(..data_size).collect::<Vec<_>>();

                        self.data_size = None;
                        self.sym = None;
                        self.typ = None;

                        return Some(Packet {
                            string_loc: sym as usize,
                            type_loc: typ as usize,
                            buffer: buf,
                        });
                    } else {
                        return None;
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}
