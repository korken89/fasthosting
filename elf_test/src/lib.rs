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

#[derive(Debug)]
pub enum BaseEncoding {
    Decimal,
    Hex,
    Octal,
    Binary,
}

// Convert DW_ATE + size into the following
#[derive(Debug)]
pub enum BaseType {
    Unsigned(usize),
    Signed(usize),
    F32,
    F64,
    Bool,
    Char,
    Zero, // Zero sized types
    Unimplemented,
}

impl BaseType {
    pub fn from_base_type(ate: gimli::DwAte, size: usize) -> Self {
        if size == 0 {
            return BaseType::Zero;
        }

        match ate {
            gimli::constants::DW_ATE_boolean => BaseType::Bool,
            gimli::constants::DW_ATE_float => {
                if size == 4 {
                    BaseType::F32
                } else if size == 8 {
                    BaseType::F64
                } else {
                    panic!("Got DW_ATE_float with size {}", size);
                }
            }
            gimli::constants::DW_ATE_signed | gimli::constants::DW_ATE_signed_char => {
                BaseType::Signed(size)
            }
            gimli::constants::DW_ATE_address
            | gimli::constants::DW_ATE_unsigned
            | gimli::constants::DW_ATE_unsigned_char => BaseType::Unsigned(size),
            gimli::constants::DW_ATE_UTF => BaseType::Unimplemented,
            gimli::constants::DW_ATE_ASCII => BaseType::Unimplemented,
            _ => BaseType::Unimplemented,
        }
    }

    /// Print buffer as base-type
    pub fn write(&self, w: &mut impl Write, buf: &[u8]) -> std::io::Result<()> {
        use BaseType::*;

        match self {
            Unsigned(size) => assert!(
                *size == buf.len(),
                "Unsigned size ({}) did not match buffer ({})",
                size,
                buf.len()
            ),
            Signed(size) => assert!(
                *size == buf.len(),
                "Signed size ({}) did not match buffer ({})",
                size,
                buf.len()
            ),
            F32 => assert!(
                4 == buf.len(),
                "f32 size ({}) did not match buffer ({})",
                4,
                buf.len()
            ),
            F64 => assert!(
                8 == buf.len(),
                "f64 size ({}) did not match buffer ({})",
                8,
                buf.len()
            ),
            Bool => assert!(
                1 == buf.len(),
                "bool size ({}) did not match buffer ({})",
                1,
                buf.len()
            ),
            Char => assert!(
                1 == buf.len(),
                "char size ({}) did not match buffer ({})",
                1,
                buf.len()
            ),
            _ => (),
        }

        match self {
            Unsigned(size) => match size {
                1 => write!(w, "{}", buf[0])?,
                2 => write!(w, "{}", u16::from_le_bytes(buf.try_into().unwrap()))?,
                4 => write!(w, "{}", u32::from_le_bytes(buf.try_into().unwrap()))?,
                8 => write!(w, "{}", u64::from_le_bytes(buf.try_into().unwrap()))?,
                16 => write!(w, "{}", u128::from_le_bytes(buf.try_into().unwrap()))?,
                _ => panic!("Unsupported size: {:#?}", self),
            },
            Signed(size) => match size {
                1 => write!(w, "{}", buf[0] as i8)?,
                2 => write!(w, "{}", i16::from_le_bytes(buf.try_into().unwrap()))?,
                4 => write!(w, "{}", i32::from_le_bytes(buf.try_into().unwrap()))?,
                8 => write!(w, "{}", i64::from_le_bytes(buf.try_into().unwrap()))?,
                16 => write!(w, "{}", i128::from_le_bytes(buf.try_into().unwrap()))?,
                _ => panic!("Unsupported size: {:#?}", self),
            },
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
                write!(w, "{}", char::from(buf[0]))?;
            }
            Zero => {}
            Unimplemented => {
                write!(w, "Unimplemented type")?;
            }
        }

        Ok(())
    }
}

// For any DWARF type it needs to become a tree of the following
#[derive(Debug)]
pub struct TypePrinter {
    // Range in buffer where the type is located
    range: Range<usize>,
    // Printer that will print the type
    printer: BaseType,
}

impl TypePrinter {
    pub fn write(&self, w: &mut impl Write, buf: &[u8]) -> std::io::Result<()> {
        self.printer.write(w, &buf.get(self.range.clone()).unwrap())
    }
}

// #[derive(Debug)]
// pub enum TypeNodePrinter {
//     Struct(Option<String>, Vec<TypeNodePrinter>),
//     Variable(Option<String>, TypePrinter),
// }

#[derive(Debug)]
pub enum PrinterTree {
    Struct(Option<String>, Vec<PrinterTree>),
    Variable(Option<String>, TypePrinter),
}

impl PrinterTree {
    pub fn from_base_type(ate: gimli::DwAte, size: usize) -> Self {
        PrinterTree::Variable(
            None,
            TypePrinter {
                range: 0..size,
                printer: BaseType::from_base_type(ate, size),
            },
        )
    }

    pub fn print(&self, buf: &[u8]) {
        let out = std::io::stdout();
        self.write_internal(&mut out.lock(), 0, buf).unwrap();
    }

    pub fn write(&self, w: &mut impl Write, buf: &[u8]) -> std::io::Result<()> {
        self.write_internal(w, 0, buf)
    }

    fn write_internal(&self, w: &mut impl Write, depth: usize, buf: &[u8]) -> std::io::Result<()> {
        let pad = " ".repeat(depth * 4);
        match self {
            PrinterTree::Struct(n, vec) => {
                if let Some(n) = n {
                    println!("{}{}: {{", &pad, n);
                } else {
                    println!("{}{{", &pad);
                }

                for t in vec {
                    t.write_internal(w, depth + 1, buf)?;
                }

                println!("{}}},", &pad);
            }
            PrinterTree::Variable(n, t) => {
                if let Some(n) = n {
                    print!("{}{}: ", &pad, n);
                } else {
                    print!("{}", &pad);
                }

                t.write(w, buf)?;

                println!(",");
            }
        }

        Ok(())
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
            printer: BaseType::Unsigned(1),
        };
        let printer2 = TypePrinter {
            range: 0..2,
            printer: BaseType::Unsigned(2),
        };
        let printer3 = TypePrinter {
            range: 0..4,
            printer: BaseType::Unsigned(4),
        };
        let printer4 = TypePrinter {
            range: 0..4,
            printer: BaseType::F32,
        };

        println!();
        printer.print(&mut out.lock(), buf).ok();
        println!();
        printer2.print(&mut out.lock(), buf).ok();
        println!();
        printer3.print(&mut out.lock(), buf).ok();
        println!();
        printer4.print(&mut out.lock(), buf).ok();
        println!();
    }

    #[test]
    fn print_tree() {
        // let mut tree = PrinterTree::new();
        // tree.add_variable("a", "123");

        // let mut tree2 = PrinterTree::new();
        // tree2.add_variable("a2", "92");

        // let mut tree3 = PrinterTree::new();
        // tree3.add_variable("a3", "12");
        // tree3.add_variable("b3", "7");
        // tree2.add_struct("tree2", tree3);

        // tree2.add_variable("b2", "93");

        // tree.add_struct("tree", tree2);

        // tree.add_variable("b", "2");
        // tree.add_variable("c", "3");
        // tree.add_variable("d", "4");

        // tree.print(&[]);
    }
}
