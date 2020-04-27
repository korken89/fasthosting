use crate::leb128;
use std::collections::VecDeque;

#[derive(Debug, PartialEq, Eq)]
pub struct Packet {
    pub string_loc: usize,
    pub type_loc: usize,
    pub buffer: Vec<u8>,
}

#[derive(Debug)]
pub struct Parser {
    buf: VecDeque<u8>,
    data_size: Option<usize>,
    sym: Option<u32>,
    typ: Option<u32>,
}

impl Parser {
    /// Create a new parser
    pub fn new() -> Self {
        Parser {
            buf: VecDeque::with_capacity(10 * 1024 * 1024),
            data_size: None,
            sym: None,
            typ: None,
        }
    }

    /// Push a slice of data into the parser
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend(data.iter());
    }

    fn try_leb128(&mut self) -> Option<u32> {
        let slices = self.buf.as_slices();
        let data_iter = slices.0.iter().chain(slices.1.iter());

        if let Ok((val, len_used)) = leb128::decode_u32(data_iter) {
            for _ in 0..len_used {
                self.buf.pop_front();
            }

            Some(val)
        } else {
            None
        }
    }

    /// Try to parse the existing buffer
    pub fn try_parse(&mut self) -> Option<Packet> {
        loop {
            match (self.data_size, self.sym, self.typ) {
                (None, _, _) => {
                    self.data_size = Some(self.try_leb128()? as usize);
                }
                (Some(_), None, _) => {
                    self.sym = Some(self.try_leb128()?);
                }
                (Some(_), Some(_), None) => {
                    self.typ = Some(self.try_leb128()?);
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
            }
        }
    }
}
