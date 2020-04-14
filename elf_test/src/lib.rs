use std::convert::TryInto;
use std::io::Write;
use std::ops::Range;

/// Extension trait for `Range` to check for overlap
pub trait ExtRange<T> {
    // Returns true if 2 ranges are overlapping
    fn is_overlapping(&self, other: &Range<T>) -> bool;
}

impl ExtRange<usize> for Range<usize> {
    fn is_overlapping(&self, other: &Range<usize>) -> bool {
        self.start.max(other.start) < self.end.min(other.end)
    }
}

pub enum BaseEncoding {
    Decimal,
    Hex,
}

// Convert DW_ATE + size into the following
pub enum BaseType {
    U8(BaseEncoding),
    U16(BaseEncoding),
    U32(BaseEncoding),
    U64(BaseEncoding),
    U128(BaseEncoding),
    I8(BaseEncoding),
    I16(BaseEncoding),
    I32(BaseEncoding),
    I64(BaseEncoding),
    I128(BaseEncoding),
    F32,
    F64,
    Bool,
    Char,
}

impl BaseType {
    // TODO

    /// Print buffer as base-type
    pub fn print(&self, w: &mut impl Write, buf: &[u8]) -> std::io::Result<()> {
        use BaseEncoding::*;
        use BaseType::*;
        match self {
            U8(Decimal) => {
                assert!(buf.len() == 1);
                write!(w, "{}", buf[0])?;
            }
            U8(Hex) => {
                assert!(buf.len() == 1);
                write!(w, "0x{:x}", buf[0])?;
            }
            U16(Decimal) => {
                write!(w, "{}", u16::from_le_bytes(buf.try_into().unwrap()))?;
            }
            U16(Hex) => {
                write!(w, "0x{:x}", u16::from_le_bytes(buf.try_into().unwrap()))?;
            }
            U32(Decimal) => {
                write!(w, "{}", u32::from_le_bytes(buf.try_into().unwrap()))?;
            }
            U32(Hex) => {
                write!(w, "0x{:x}", u32::from_le_bytes(buf.try_into().unwrap()))?;
            }
            U64(Decimal) => {
                write!(w, "{}", u64::from_le_bytes(buf.try_into().unwrap()))?;
            }
            U64(Hex) => {
                write!(w, "0x{:x}", u64::from_le_bytes(buf.try_into().unwrap()))?;
            }
            U128(Decimal) => {
                write!(w, "{}", u128::from_le_bytes(buf.try_into().unwrap()))?;
            }
            U128(Hex) => {
                write!(w, "0x{:x}", u128::from_le_bytes(buf.try_into().unwrap()))?;
            }
            I8(Decimal) => {
                assert!(buf.len() == 1);
                write!(w, "{}", buf[0] as i8)?;
            }
            I8(Hex) => {
                assert!(buf.len() == 1);
                write!(w, "0x{:x}", buf[0] as i8)?;
            }
            I16(Decimal) => {
                write!(w, "{}", i16::from_le_bytes(buf.try_into().unwrap()))?;
            }
            I16(Hex) => {
                write!(w, "0x{:x}", i16::from_le_bytes(buf.try_into().unwrap()))?;
            }
            I32(Decimal) => {
                write!(w, "{}", i32::from_le_bytes(buf.try_into().unwrap()))?;
            }
            I32(Hex) => {
                write!(w, "0x{:x}", i32::from_le_bytes(buf.try_into().unwrap()))?;
            }
            I64(Decimal) => {
                write!(w, "{}", i64::from_le_bytes(buf.try_into().unwrap()))?;
            }
            I64(Hex) => {
                write!(w, "0x{:x}", i64::from_le_bytes(buf.try_into().unwrap()))?;
            }
            I128(Decimal) => {
                write!(w, "{}", i128::from_le_bytes(buf.try_into().unwrap()))?;
            }
            I128(Hex) => {
                write!(w, "0x{:x}", i128::from_le_bytes(buf.try_into().unwrap()))?;
            }
            F32 => {
                write!(w, "{}", f32::from_le_bytes(buf.try_into().unwrap()))?;
            }
            F64 => {
                write!(w, "{}", f64::from_le_bytes(buf.try_into().unwrap()))?;
            }
            Bool => {
                assert!(buf.len() == 1);
                if buf[0] == 0 {
                    write!(w, "false")?;
                } else if buf[0] == 1 {
                    write!(w, "true")?;
                } else {
                    panic!("not a bool: {}", buf[0]);
                }
            }
            Char => {
                write!(
                    w,
                    "{}",
                    std::char::from_u32(u32::from_le_bytes(buf.try_into().unwrap())).unwrap()
                )?;
            }
        }

        Ok(())
    }

    /// Print buffer as array of base-type
    pub fn print_array(&self, _w: &mut impl Write, _buf: &[u8]) -> std::io::Result<()> {
        todo!();
    }
}

pub enum Printer {
    Single(BaseType),
    Array(BaseType),
    // Custom(()), // TODO
}

impl Printer {
    // TODO
    pub fn print(&self, w: &mut impl Write, buf: &[u8]) -> std::io::Result<()> {
        match self {
            Printer::Single(t) => t.print(w, buf),
            Printer::Array(t) => t.print_array(w, buf),
        }
    }
}

// For any DWARF type it needs to become a tree of the following
pub struct TypePrinter<'a> {
    // Range in buffer where the type is located
    pub(crate) range: Range<usize>,
    // Printer that will print the type
    pub(crate) printer: Printer,
    // Original buffer
    buf: &'a [u8],
}

impl<'a> TypePrinter<'a> {
    pub fn print(&self, w: &mut impl Write) -> std::io::Result<()> {
        self.printer
            .print(w, &self.buf.get(self.range.clone()).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn range_overlap_1() {
        let r1 = 0..3;
        let r2 = 2..10;

        assert!(r1.is_overlapping(&r2));
    }

    #[test]
    fn range_not_overlap_1() {
        let r1 = 0..3;
        let r2 = 3..10;

        assert!(!r1.is_overlapping(&r2));
    }

    #[test]
    fn print() {
        let out = std::io::stdout();

        let buf = &[1, 0, 0, 7];
        let printer = TypePrinter {
            range: 0..1,
            printer: Printer::Single(BaseType::U8(BaseEncoding::Decimal)),
            buf,
        };
        let printer2 = TypePrinter {
            range: 0..2,
            printer: Printer::Single(BaseType::U16(BaseEncoding::Hex)),
            buf,
        };
        let printer3 = TypePrinter {
            range: 0..4,
            printer: Printer::Single(BaseType::U32(BaseEncoding::Hex)),
            buf,
        };

        println!();
        printer.print(&mut out.lock()).ok();
        println!();
        printer2.print(&mut out.lock()).ok();
        println!();
        printer3.print(&mut out.lock()).ok();
        println!();
    }
}
