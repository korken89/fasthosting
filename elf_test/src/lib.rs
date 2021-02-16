use gimli::{
    constants, AttributeValue, DebuggingInformationEntry, DwAte, Dwarf, EntriesTreeNode, Reader,
};
use object::{Object, ObjectSection};
use std::{borrow, io::Write};
use std::{collections::HashMap, convert::TryInto};
use std::{ops::Range, rc::Rc};

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
#[derive(Debug, Clone)]
pub enum BaseType {
    Unsigned(usize),
    Signed(usize),
    F32,
    F64,
    Bool,
    Char,
    Zero(String), // Zero sized types
    Unimplemented,
}

impl BaseType {
    pub fn from_base_type(ate: DwAte, name: &str, size: usize) -> Self {
        if size == 0 {
            return BaseType::Zero(name.into());
        }

        use constants as c;

        match ate {
            c::DW_ATE_boolean => BaseType::Bool,
            c::DW_ATE_float => {
                if size == 4 {
                    BaseType::F32
                } else if size == 8 {
                    BaseType::F64
                } else {
                    panic!("Got DW_ATE_float with size {}", size);
                }
            }
            c::DW_ATE_signed | c::DW_ATE_signed_char => BaseType::Signed(size),
            c::DW_ATE_address | c::DW_ATE_unsigned | c::DW_ATE_unsigned_char => {
                BaseType::Unsigned(size)
            }
            c::DW_ATE_UTF => BaseType::Unimplemented,
            c::DW_ATE_ASCII => BaseType::Unimplemented,
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
            Zero(s) => {
                write!(w, "{}", &s)?;
            }
            Unimplemented => {
                write!(w, "Unimplemented type")?;
            }
        }

        Ok(())
    }
}

// For any DWARF type it needs to become a tree of the following
#[derive(Debug, Clone)]
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

#[derive(Debug)]
pub struct TypePrinters(pub HashMap<String, Type>);

impl TypePrinters {
    pub fn print(&self, type_name: &str, buffer: &[u8]) {
        println!("{}", type_name);
        if let Some(typ) = self.0.get(type_name) {
            let mut out = std::io::stdout();
            let _ = typ.write(&mut out, buffer);
        }
    }
}

#[derive(Debug, Clone)]
pub struct Struct {
    pub named_children: std::collections::HashMap<String, Type>,
    pub indexed_children: Vec<Type>,
}

#[derive(Debug, Clone)]
pub struct Enum {
    pub variants: std::collections::HashMap<String, Type>,
    pub discriminant_offset: usize,
}

#[derive(Debug, Clone)]
pub struct Scalar {
    pub printer: TypePrinter,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    Struct(Struct),
    Enum(Enum),
    Scalar(Scalar),
    Pointer(Box<Type>),
    PlainVariant,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Type {
    pub offset: usize,
    kind: TypeKind,
    name: String,
    namespace: Vec<String>,
    pub variant_value: usize,
}

impl TypeKind {
    pub fn new_from_base_type(ate: DwAte, name: &str, size: usize) -> Self {
        TypeKind::Scalar(Scalar {
            printer: TypePrinter {
                range: 0..size,
                printer: BaseType::from_base_type(ate, name, size),
            },
        })
    }
}

impl Type {
    pub fn new(kind: TypeKind, name: String, namespace: Vec<String>, offset: usize) -> Self {
        Self {
            kind,
            name,
            namespace,
            offset,
            variant_value: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn write(&self, w: &mut impl Write, buf: &[u8]) -> std::io::Result<()> {
        self.write_internal(w, 0, buf)
    }

    fn write_internal(&self, w: &mut impl Write, depth: usize, buf: &[u8]) -> std::io::Result<()> {
        let pad = " ".repeat(depth * 4);
        match &self.kind {
            TypeKind::Struct(structure) => {
                if !structure.named_children.is_empty() {
                    println!("{}{}: {{", &pad, self.name);

                    for (_name, typ) in &structure.named_children {
                        typ.write_internal(w, depth + 1, &buf[self.offset..])?;
                    }

                    println!("{}}},", &pad);
                } else if !structure.indexed_children.is_empty() {
                    println!("{}{}: (", &pad, self.name);

                    for (i, typ) in structure.indexed_children.iter().enumerate() {
                        typ.write_internal(w, depth + 1, &buf[self.offset..])?;
                    }

                    println!("{}),", &pad);
                }
            }
            TypeKind::Enum(enummeration) => {
                print!("{}{}::", &pad, self.name);
                let discriminant = buf[enummeration.discriminant_offset] as usize;
                for (variant_name, variant) in &enummeration.variants {
                    if variant.variant_value == discriminant {
                        if let TypeKind::PlainVariant = variant.kind {
                            println!("{}", variant_name);
                        } else {
                            println!("{} {{", variant_name);
                            variant.write_internal(w, depth + 1, &buf[self.offset..])?;
                            println!("}}");
                        }
                    }
                }
                // if let Some(n) = n {
                //     println!("{}{}: {{", &pad, n);
                // } else {
                //     println!("{}{{", &pad);
                // }

                // for t in vec {
                //     t.write_internal(w, depth + 1, buf)?;
                // }

                // println!("{}}},", &pad);
            }
            TypeKind::Scalar(scalar) => {
                print!("{}{}: ", &pad, self.name);

                scalar.printer.write(w, &buf[self.offset..])?;

                println!(",");
            }
            TypeKind::PlainVariant => {
                print!("{}{}: ", &pad, self.name);

                println!(",");
            }
            TypeKind::Pointer(typ) => {
                print!("*");
                typ.write_internal(w, depth, buf)?;
            }
            TypeKind::Unknown => (),
        }

        Ok(())
    }
}

pub fn generate_printers(elf: &[u8]) -> Result<TypePrinters, anyhow::Error> {
    // Namespace tracker
    let mut _namespace_tracker: Vec<String> = Vec::new();

    // Where printers are stored
    let mut printers: HashMap<String, Type> = HashMap::new();

    let debug_info = DebugInfo::from_raw(elf).unwrap();
    let mut units = debug_info.get_units();
    while let Some(unit_info) = debug_info.get_next_unit_info(&mut units) {
        let types = unit_info.list_types().unwrap();
        printers.extend(types.into_iter().map(|t| (t.name().to_string(), t)));
    }

    Ok(TypePrinters(printers))
}

/// Helper types to reduce signature bloat.
type R = gimli::EndianReader<gimli::LittleEndian, std::rc::Rc<[u8]>>;
type DwarfReader = gimli::read::EndianRcSlice<gimli::LittleEndian>;
type UnitIter =
    gimli::DebugInfoUnitHeadersIter<gimli::EndianReader<gimli::LittleEndian, std::rc::Rc<[u8]>>>;
type NamespaceDie<'abbrev, 'unit> = gimli::DebuggingInformationEntry<
    'abbrev,
    'unit,
    gimli::EndianReader<gimli::LittleEndian, std::rc::Rc<[u8]>>,
    usize,
>;
type EntriesCursor<'abbrev, 'unit> = gimli::EntriesCursor<
    'abbrev,
    'unit,
    gimli::EndianReader<gimli::LittleEndian, std::rc::Rc<[u8]>>,
>;

/// This struct contains all the necessary debug info we might need during our traversal.
pub struct DebugInfo {
    dwarf: gimli::Dwarf<DwarfReader>,
    _frame_section: gimli::DebugFrame<DwarfReader>,
}

impl DebugInfo {
    /// Parse debug information directly from a buffer containing an ELF file.
    fn from_raw(data: &[u8]) -> Result<Self, ()> {
        let object = object::File::parse(data).unwrap();

        // Load a section and return as `Cow<[u8]>`.
        let load_section = |id: gimli::SectionId| -> Result<DwarfReader, gimli::Error> {
            let data = object
                .section_by_name(id.name())
                .and_then(|section| section.uncompressed_data().ok())
                .unwrap_or_else(|| borrow::Cow::Borrowed(&[][..]));

            Ok(gimli::read::EndianRcSlice::new(
                Rc::from(&*data),
                gimli::LittleEndian,
            ))
        };
        // Load a supplementary section. We don't have a supplementary object file,
        // so always return an empty slice.
        let load_section_sup = |_| {
            Ok(gimli::read::EndianRcSlice::new(
                Rc::from(&*borrow::Cow::Borrowed(&[][..])),
                gimli::LittleEndian,
            ))
        };

        // Load all of the sections.
        let dwarf_cow = gimli::Dwarf::load(&load_section, &load_section_sup).unwrap();

        use gimli::Section;
        let mut frame_section = gimli::DebugFrame::load(load_section).unwrap();

        // To support DWARF v2, where the address size is not encoded in the .debug_frame section,
        // we have to set the address size here.
        frame_section.set_address_size(4);

        Ok(DebugInfo {
            //object,
            dwarf: dwarf_cow,
            _frame_section: frame_section,
        })
    }

    /// Returns an iterator over all the units in the currently open DWARF blob.
    fn get_units(&self) -> UnitIter {
        self.dwarf.units()
    }

    /// Get the next unit in the unit iterator given.
    fn get_next_unit_info(&self, units: &mut UnitIter) -> Option<UnitInfo> {
        while let Ok(Some(header)) = units.next() {
            if let Ok(unit) = self.dwarf.unit(header) {
                return Some(UnitInfo {
                    debug_info: self,
                    unit,
                });
            };
        }
        None
    }
}

struct UnitInfo<'debuginfo> {
    debug_info: &'debuginfo DebugInfo,
    unit: gimli::Unit<gimli::EndianReader<gimli::LittleEndian, std::rc::Rc<[u8]>>, usize>,
}

impl<'debuginfo, 'abbrev, 'unit> UnitInfo<'debuginfo> {
    /// Extracts the string representation of any string in the DWARF blob.
    /// This is mostly used to extract names of DIEs.
    fn extract_string_of(&self, attr: &gimli::Attribute<R>) -> Option<String> {
        match attr.value() {
            gimli::AttributeValue::DebugStrRef(name_ref) => {
                let name_raw = self.debug_info.dwarf.string(name_ref).unwrap();
                Some(String::from_utf8_lossy(&name_raw).to_string())
            }
            gimli::AttributeValue::String(name) => Some(String::from_utf8_lossy(&name).to_string()),
            _ => None,
        }
    }

    fn list_types(&self) -> Result<Vec<Type>, ()> {
        let mut tree = self.unit.entries_tree(None).unwrap();
        let root = tree.root().unwrap();
        self.walk_namespace(root, vec![])
    }

    fn walk_namespace(
        &self,
        node: EntriesTreeNode<R>,
        mut current_namespace: Vec<String>,
    ) -> Result<Vec<Type>, ()> {
        let mut tree = self.unit.entries_tree(Some(node.entry().offset())).unwrap();
        let root = tree.root().unwrap();
        // let namespace =
        //     self.extract_string_of(&root.entry().attr(gimli::DW_AT_name).unwrap().unwrap());
        let mut types = self.get_types(root, current_namespace.clone()).unwrap();
        let mut children = node.children();
        while let Ok(Some(child)) = children.next() {
            let entry = child.entry();
            match entry.tag() {
                gimli::DW_TAG_namespace => {
                    let mut name = String::new();
                    let mut attrs = entry.attrs();
                    while let Ok(Some(attr)) = attrs.next() {
                        match attr.name() {
                            gimli::DW_AT_name => {
                                name = self
                                    .extract_string_of(&attr)
                                    .unwrap_or_else(|| "<undefined>".to_string());
                            }
                            _ => (),
                        }
                    }
                    current_namespace.push(name);

                    types.extend(
                        self.walk_namespace(child, current_namespace.clone())
                            .unwrap(),
                    )
                }
                _ => (),
            };
        }

        Ok(types)
    }

    /// Returns the type that `node` represents.
    fn extract_type_of(
        &self,
        node: EntriesTreeNode<R>,
        current_namespace: Vec<String>,
        offset: usize,
    ) -> Option<Type> {
        // Examine the entry attributes.
        let entry = node.entry();
        match entry.tag() {
            gimli::DW_TAG_structure_type => {
                let type_name =
                    self.extract_string_of(&entry.attr(gimli::DW_AT_name).unwrap().unwrap());
                if !node.entry().has_children() {
                    return Some(Type::new(
                        TypeKind::PlainVariant,
                        type_name.unwrap_or_else(|| "<unnamed type>".to_string()),
                        current_namespace.clone(),
                        offset,
                    ));
                }
                let mut named_children = std::collections::HashMap::new();
                let mut indexed_children = Vec::new();
                let mut variants = std::collections::HashMap::new();
                let mut discriminant_offset: usize = 0;

                let mut children = node.children();
                while let Ok(Some(child)) = children.next() {
                    let entry = child.entry();
                    println!("{}", entry.tag());
                    match entry.tag() {
                        gimli::DW_TAG_member => {
                            let mut attrs = entry.attrs();
                            while let Ok(Some(attr)) = attrs.next() {
                                match attr.name() {
                                    _attr => println!("{}", _attr),
                                }
                            }
                            let (name, typ) =
                                self.extract_member_of(child, current_namespace.clone(), offset);
                            if name.starts_with("__") {
                                let index = name.strip_prefix("__").unwrap().parse().unwrap();
                                indexed_children.insert(
                                    index,
                                    typ.unwrap_or(Type::new(
                                        TypeKind::Unknown,
                                        index.to_string(),
                                        current_namespace.clone(),
                                        offset,
                                    )),
                                );
                            } else {
                                named_children.insert(
                                    name.clone(),
                                    typ.unwrap_or(Type::new(
                                        TypeKind::Unknown,
                                        name,
                                        current_namespace.clone(),
                                        offset,
                                    )),
                                );
                            }
                        }
                        gimli::DW_TAG_variant_part => {
                            let mut attrs = entry.attrs();
                            while let Ok(Some(attr)) = attrs.next() {
                                match attr.name() {
                                    gimli::DW_AT_discr => {
                                        let mut tree = self
                                            .unit
                                            .entries_tree(Some(match attr.value() {
                                                AttributeValue::UnitRef(v) => v,
                                                _ => panic!(),
                                            }))
                                            .unwrap();
                                        let discriminant = tree.root().unwrap();
                                        let discriminant = discriminant.entry();

                                        let mut attrs = discriminant.attrs();
                                        while let Ok(Some(attr)) = attrs.next() {
                                            match attr.name() {
                                                gimli::DW_AT_data_member_location => {
                                                    if let AttributeValue::Udata(s) = attr.value() {
                                                        discriminant_offset = s.try_into().unwrap();
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    _attr => println!("{}", _attr),
                                }
                            }

                            let mut children = child.children();
                            while let Ok(Some(child)) = children.next() {
                                let entry = child.entry();

                                if entry.tag() == gimli::DW_TAG_variant {
                                    println!("VARIANT");
                                    let mut discriminant_value: usize = 0;
                                    let mut attrs = entry.attrs();
                                    while let Ok(Some(attr)) = attrs.next() {
                                        match attr.name() {
                                            gimli::DW_AT_discr_value => {
                                                println!("XXX");
                                                if let AttributeValue::Data1(s) = attr.value() {
                                                    discriminant_value = s.try_into().unwrap();
                                                    println!("{}", discriminant_value);
                                                }
                                            }
                                            _ => {}
                                        }
                                    }

                                    let mut children = child.children();
                                    while let Ok(Some(child)) = children.next() {
                                        let mut variant_offset: usize = 0;
                                        let mut name = String::new();
                                        let entry = child.entry();

                                        if entry.tag() == gimli::DW_TAG_member {
                                            let mut type_attr = None;
                                            let mut attrs = entry.attrs();
                                            while let Ok(Some(attr)) = attrs.next() {
                                                match attr.name() {
                                                    gimli::DW_AT_data_member_location => {
                                                        if let AttributeValue::Udata(s) =
                                                            attr.value()
                                                        {
                                                            variant_offset = s.try_into().unwrap();
                                                        }
                                                    }
                                                    gimli::DW_AT_name => {
                                                        name = self
                                                            .extract_string_of(&attr)
                                                            .unwrap_or_else(|| {
                                                                "<undefined>".to_string()
                                                            });
                                                    }
                                                    gimli::DW_AT_type => type_attr = Some(attr),
                                                    _ => {}
                                                }
                                            }

                                            if let Some(type_attr) = type_attr {
                                                let mut tree = self
                                                    .unit
                                                    .entries_tree(Some(match type_attr.value() {
                                                        AttributeValue::UnitRef(v) => v,
                                                        _ => panic!(),
                                                    }))
                                                    .unwrap();
                                                let root = tree.root().unwrap();
                                                variants.insert(
                                                    name.clone(),
                                                    self.extract_type_of(
                                                        root,
                                                        current_namespace.clone(),
                                                        variant_offset,
                                                    )
                                                    .map(|mut t| {
                                                        t.variant_value = discriminant_value;
                                                        t
                                                    })
                                                    .unwrap_or(Type::new(
                                                        TypeKind::Unknown,
                                                        name,
                                                        current_namespace.clone(),
                                                        offset,
                                                    )),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _tag => {}
                    };
                }

                if !named_children.is_empty() {
                    return Some(Type::new(
                        TypeKind::Struct(Struct {
                            named_children,
                            indexed_children,
                        }),
                        type_name.unwrap_or_else(|| "<unnamed type>".to_string()),
                        current_namespace,
                        offset,
                    ));
                } else if !indexed_children.is_empty() {
                    return Some(Type::new(
                        TypeKind::Struct(Struct {
                            named_children,
                            indexed_children,
                        }),
                        type_name.unwrap_or_else(|| "<unnamed type>".to_string()),
                        current_namespace,
                        offset,
                    ));
                } else if !variants.is_empty() {
                    return Some(Type::new(
                        TypeKind::Enum(Enum {
                            variants,
                            discriminant_offset,
                        }),
                        type_name.unwrap_or_else(|| "<unnamed type>".to_string()),
                        current_namespace,
                        offset,
                    ));
                }
            }
            gimli::DW_TAG_base_type => {
                if let Ok(Some((name, enc, size))) =
                    get_base_type_info(&self.debug_info.dwarf, &entry)
                {
                    return Some(Type::new(
                        TypeKind::new_from_base_type(enc, &name, size),
                        name,
                        current_namespace,
                        offset,
                    ));
                }
            }
            t => println!("Unknown type class: {}", t),
        };

        return None;
    }

    /// Returns the member that `node` represents.
    fn extract_member_of(
        &self,
        node: EntriesTreeNode<R>,
        current_namespace: Vec<String>,
        mut offset: usize,
    ) -> (String, Option<Type>) {
        let mut name = "".into();
        let mut attrs = node.entry().attrs();
        let mut type_attr = None;
        while let Ok(Some(attr)) = attrs.next() {
            match attr.name() {
                gimli::DW_AT_name => {
                    name = self
                        .extract_string_of(&attr)
                        .unwrap_or_else(|| "<undefined>".to_string());
                }
                gimli::DW_AT_type => {
                    type_attr = Some(attr);
                }
                constants::DW_AT_data_member_location => {
                    if let AttributeValue::Udata(s) = attr.value() {
                        offset = s.try_into().unwrap();
                    }
                }
                _attr => println!("{}", _attr),
            }
        }

        let typ = if let Some(type_attr) = type_attr {
            dbg!(type_attr.value());
            let mut tree = self
                .unit
                .entries_tree(Some(match type_attr.value() {
                    AttributeValue::UnitRef(v) => v,
                    _ => return (String::new(), None),
                }))
                .unwrap();
            let root = tree.root().unwrap();
            self.extract_type_of(root, current_namespace.clone(), offset)
        } else {
            panic!();
        };

        return (name, typ);
    }

    /// Returns all the variables in the current DIE.
    fn _get_variables(&self, die_cursor_state: &mut DieCursorState) -> Result<Vec<Variable>, ()> {
        let mut variables = vec![];

        die_cursor_state.entries_cursor.next_dfs().unwrap();

        while let Some(current) = die_cursor_state.entries_cursor.next_sibling().unwrap() {
            if let gimli::DW_TAG_variable = current.tag() {
                let mut variable = Variable {
                    name: String::new(),
                    file: String::new(),
                    line: u64::max_value(),
                    value: 0,
                    typ: Type::new(TypeKind::Unknown, String::new(), vec![], 0),
                };
                let mut attrs = current.attrs();
                while let Ok(Some(attr)) = attrs.next() {
                    match attr.name() {
                        gimli::DW_AT_name => {
                            variable.name = self
                                .extract_string_of(&attr)
                                .unwrap_or_else(|| "<undefined>".to_string());
                        }
                        _ => (),
                    }
                }
                variables.push(variable);
            };
        }

        Ok(variables)
    }

    /// Returns all the types in the current DIE.
    fn get_types(
        &self,
        node: EntriesTreeNode<R>,
        current_namespace: Vec<String>,
    ) -> Result<Vec<Type>, ()> {
        let mut types = vec![];

        let mut children = node.children();

        while let Ok(Some(current)) = children.next() {
            if let gimli::DW_TAG_structure_type = current.entry().tag() {
                if let Some(typ) = self.extract_type_of(current, current_namespace.clone(), 0) {
                    types.push(typ);
                }
            };
        }

        Ok(types)
    }
}

#[derive(Debug)]
pub struct Variable {
    pub name: String,
    pub file: String,
    pub line: u64,
    pub value: u64,
    pub typ: Type,
}

#[derive(Clone)]
struct DieCursorState<'abbrev, 'unit> {
    entries_cursor: EntriesCursor<'abbrev, 'unit>,
    _depth: isize,
    name: String,
    namespace_die: NamespaceDie<'abbrev, 'unit>,
}

#[allow(dead_code)]
fn get_type<T>(_: &T) -> &'static str {
    core::any::type_name::<T>()
}

fn get_base_type_info<T: Reader>(
    dwarf: &Dwarf<T>,
    die_entry: &DebuggingInformationEntry<T>,
) -> Result<Option<(String, DwAte, usize)>, anyhow::Error> {
    let mut name: Option<String> = None;
    let mut encoding: Option<DwAte> = None;
    let mut size: Option<usize> = None;

    let mut attrs = die_entry.attrs();
    while let Some(attr) = attrs.next()? {
        // Find name
        match attr.name() {
            constants::DW_AT_name => {
                if let AttributeValue::DebugStrRef(r) = attr.value() {
                    if let Ok(s) = dwarf.string(r) {
                        if let Ok(s) = s.to_string() {
                            name = Some(s.into());
                        }
                    }
                }
            }

            // Find encoding
            constants::DW_AT_encoding => {
                if let AttributeValue::Encoding(enc) = attr.value() {
                    encoding = Some(enc);
                }
            }

            // Find size
            constants::DW_AT_byte_size => {
                if let AttributeValue::Udata(s) = attr.value() {
                    size = Some(s.try_into()?);
                }
            }
            _attr => (),
        }
    }

    if let (Some(name), Some(enc), Some(size)) = (name, encoding, size) {
        Ok(Some((name, enc, size)))
    } else {
        Ok(None)
    }
}

trait DwTagExt {
    fn is_base_type(&self) -> bool;
    fn is_complex_type(&self) -> bool;
}

impl DwTagExt for gimli::DwTag {
    fn is_base_type(&self) -> bool {
        use constants as c;
        match *self {
            c::DW_TAG_base_type => true,
            _ => false,
        }
    }

    fn is_complex_type(&self) -> bool {
        use constants as c;
        match *self {
            c::DW_TAG_structure_type
            | c::DW_TAG_enumeration_type
            | c::DW_TAG_union_type
            | c::DW_TAG_array_type
            | c::DW_TAG_reference_type
            | c::DW_TAG_string_type => true,
            _ => false,
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
        printer.write(&mut out.lock(), buf).ok();
        println!();
        printer2.write(&mut out.lock(), buf).ok();
        println!();
        printer3.write(&mut out.lock(), buf).ok();
        println!();
        printer4.write(&mut out.lock(), buf).ok();
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
