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
    // Octal,
    // Binary,
}

// Convert DW_ATE + size into the following
#[derive(Debug)]
pub enum BaseType {
    Unsigned(usize, BaseEncoding),
    Signed(usize, BaseEncoding),
    F32,
    F64,
    Bool,
    Char,
    Unimplemented,
}

impl BaseType {
    // TODO

    /// Print buffer as base-type
    pub fn print(&self, w: &mut impl Write, buf: &[u8]) -> std::io::Result<()> {
        use BaseEncoding::*;
        use BaseType::*;

        match self {
            Unsigned(size, _) => assert!(
                *size == buf.len(),
                "Unsigned size ({}) did not match buffer ({})",
                size,
                buf.len()
            ),
            Signed(size, _) => assert!(
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
                4 == buf.len(),
                "char size ({}) did not match buffer ({})",
                4,
                buf.len()
            ),
            _ => (),
        }

        match self {
            Unsigned(size, Decimal) => match size {
                1 => write!(w, "{}", buf[0])?,
                2 => write!(w, "{}", u16::from_le_bytes(buf.try_into().unwrap()))?,
                4 => write!(w, "{}", u32::from_le_bytes(buf.try_into().unwrap()))?,
                8 => write!(w, "{}", u64::from_le_bytes(buf.try_into().unwrap()))?,
                16 => write!(w, "{}", u128::from_le_bytes(buf.try_into().unwrap()))?,
                _ => panic!("Unsupported size: {:#?}", self),
            },
            Unsigned(size, Hex) => match size {
                1 => write!(w, "0x{:x}", buf[0])?,
                2 => write!(w, "0x{:x}", u16::from_le_bytes(buf.try_into().unwrap()))?,
                4 => write!(w, "0x{:x}", u32::from_le_bytes(buf.try_into().unwrap()))?,
                8 => write!(w, "0x{:x}", u64::from_le_bytes(buf.try_into().unwrap()))?,
                16 => write!(w, "0x{:x}", u128::from_le_bytes(buf.try_into().unwrap()))?,
                _ => panic!("Unsupported size: {:#?}", self),
            },
            Signed(size, Decimal) => match size {
                1 => write!(w, "{}", buf[0] as i8)?,
                2 => write!(w, "{}", i16::from_le_bytes(buf.try_into().unwrap()))?,
                4 => write!(w, "{}", i32::from_le_bytes(buf.try_into().unwrap()))?,
                8 => write!(w, "{}", i64::from_le_bytes(buf.try_into().unwrap()))?,
                16 => write!(w, "{}", i128::from_le_bytes(buf.try_into().unwrap()))?,
                _ => panic!("Unsupported size: {:#?}", self),
            },
            Signed(size, Hex) => match size {
                1 => write!(w, "0x{:x}", buf[0] as i8)?,
                2 => write!(w, "0x{:x}", i16::from_le_bytes(buf.try_into().unwrap()))?,
                4 => write!(w, "0x{:x}", i32::from_le_bytes(buf.try_into().unwrap()))?,
                8 => write!(w, "0x{:x}", i64::from_le_bytes(buf.try_into().unwrap()))?,
                16 => write!(w, "0x{:x}", i128::from_le_bytes(buf.try_into().unwrap()))?,
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
                write!(
                    w,
                    "{}",
                    std::char::from_u32(u32::from_le_bytes(buf.try_into().unwrap())).unwrap()
                )?;
            }
            Unimplemented => {
                write!(w, "Unimplemented type",)?;
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
    // Enum // TODO
    // Custom(()), // TODO
}

impl Printer {
    // TODO
    fn from_base_type(ate: gimli::DwAte, size: usize) -> Self {
        match ate {
            gimli::constants::DW_ATE_address => Printer::Single(BaseType::Unimplemented),
            gimli::constants::DW_ATE_boolean => Printer::Single(BaseType::Unimplemented),
            gimli::constants::DW_ATE_float => Printer::Single(BaseType::Unimplemented),
            gimli::constants::DW_ATE_signed => Printer::Single(BaseType::Unimplemented),
            gimli::constants::DW_ATE_signed_char => Printer::Single(BaseType::Unimplemented),
            gimli::constants::DW_ATE_unsigned => Printer::Single(BaseType::Unimplemented),
            gimli::constants::DW_ATE_unsigned_char => Printer::Single(BaseType::Unimplemented),
            gimli::constants::DW_ATE_UTF => Printer::Single(BaseType::Unimplemented),
            gimli::constants::DW_ATE_ASCII => Printer::Single(BaseType::Unimplemented),
            _ => Printer::Single(BaseType::Unimplemented),
            // gimli::constants::DW_ATE_complex_float => Printer::Single(BaseType::Unimplemented),
            // gimli::constants::DW_ATE_imaginary_float => Printer::Single(BaseType::Unimplemented),
            // gimli::constants::DW_ATE_packed_decimal => Printer::Single(BaseType::Unimplemented),
            // gimli::constants::DW_ATE_numeric_string => Printer::Single(BaseType::Unimplemented),
            // gimli::constants::DW_ATE_edited => Printer::Single(BaseType::Unimplemented),
            // gimli::constants::DW_ATE_signed_fixed => Printer::Single(BaseType::Unimplemented),
            // gimli::constants::DW_ATE_unsigned_fixed => Printer::Single(BaseType::Unimplemented),
            // gimli::constants::DW_ATE_decimal_float => Printer::Single(BaseType::Unimplemented),
            // gimli::constants::DW_ATE_UCS => Printer::Single(BaseType::Unimplemented),
            // gimli::constants::DW_ATE_lo_user => Printer::Single(BaseType::Unimplemented),
            // gimli::constants::DW_ATE_hi_user => Printer::Single(BaseType::Unimplemented),
        }
    }

    pub fn print(&self, w: &mut impl Write, buf: &[u8]) -> std::io::Result<()> {
        match self {
            Printer::Single(t) => t.print(w, buf),
            Printer::Array(t) => t.print_array(w, buf),
        }
    }
}

// For any DWARF type it needs to become a tree of the following
pub struct TypePrinter {
    // Range in buffer where the type is located
    pub(crate) range: Range<usize>,
    // Printer that will print the type
    pub(crate) printer: Printer,
}

impl TypePrinter {
    pub fn print(&self, w: &mut impl Write, buf: &[u8]) -> std::io::Result<()> {
        self.printer.print(w, &buf.get(self.range.clone()).unwrap())
    }
}

pub enum TypeNode<'a> {
    Branch(&'a str, Box<PrinterTree<'a>>),
    Leaf(&'a str, TypePrinter),
}

pub struct PrinterTree<'a> {
    nodes: Vec<TypeNode<'a>>,
}

impl<'a> PrinterTree<'a> {
    pub fn new() -> Self {
        PrinterTree { nodes: Vec::new() }
    }

    pub fn add_printer(&mut self, name: &'a str, p: TypePrinter) {
        self.nodes.push(TypeNode::Leaf(name, p));
    }

    pub fn add_branch(&mut self, name: &'a str, l: PrinterTree<'a>) {
        self.nodes.push(TypeNode::Branch(name, l.into()));
    }

    pub fn print(&self, buf: &[u8]) {
        self.print_internal(0, buf);
    }

    fn print_internal(&self, depth: usize, buf: &[u8]) {
        let pad = " ".repeat(depth * 4);
        for v in &self.nodes {
            match v {
                TypeNode::Branch(n, t) => {
                    println!("{}{}: {{", &pad, n);
                    t.print_internal(depth + 1, buf);
                    println!("{}}},", &pad);
                }
                TypeNode::Leaf(n, t) => {
                    print!("{}{}: ", &pad, n);
                    // t.print();
                    println!(",");
                }
            }
        }
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
            printer: Printer::Single(BaseType::Unsigned(1, BaseEncoding::Decimal)),
        };
        let printer2 = TypePrinter {
            range: 0..2,
            printer: Printer::Single(BaseType::Unsigned(2, BaseEncoding::Hex)),
        };
        let printer3 = TypePrinter {
            range: 0..4,
            printer: Printer::Single(BaseType::Unsigned(4, BaseEncoding::Hex)),
        };
        let printer4 = TypePrinter {
            range: 0..4,
            printer: Printer::Single(BaseType::F32),
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
        // tree.add_printer("a".into(), FakeTypes::U8(1));

        // let mut tree2 = PrinterTree::new();
        // tree2.add_printer("a2".into(), FakeTypes::U16(92));

        // let mut tree3 = PrinterTree::new();
        // tree3.add_printer("a3".into(), FakeTypes::U16(12));
        // tree3.add_printer("b3".into(), FakeTypes::U32(7));
        // tree2.add_branch("tree2".into(), tree3);

        // tree2.add_printer("b2".into(), FakeTypes::U32(93));

        // tree.add_branch("tree".into(), tree2);

        // tree.add_printer("b".into(), FakeTypes::U16(2));
        // tree.add_printer("c".into(), FakeTypes::U32(3));
        // tree.add_printer("d".into(), FakeTypes::U64(4));

        // tree.print(&[]);
    }
}
