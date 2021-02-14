use elf_test::{Enum, Struct, Type};
use gimli::{
    self, constants,
    read::{AttributeValue, DebuggingInformationEntry, Reader},
    DwAte, Dwarf, EntriesTreeNode,
};
use object::{Object, ObjectSection};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::{borrow, convert::TryInto, rc::Rc};
use structopt::StructOpt;

#[derive(StructOpt)]
struct Opts {
    #[structopt(name = "FILE", parse(from_os_str))]
    elf: PathBuf,
}

//
// What do we want?
// ----------------
//
// A structure for type lookup which when given a buffer can print the type.
//
// Eg:
//
// fn generate_printers(elf: &ElfFile) -> Wrapper(HashMap<TypeString, Printer>) { ... }
//
// where Wrapper.print("app::my_type", &buf) will print the type based on the data in buf
//

pub struct TypePrinters(HashMap<String, Type>);

impl TypePrinters {
    pub fn print(&self, _type_string: &str, _buffer: &[u8]) {
        todo!()
    }
}

fn generate_printers(elf: &[u8]) -> Result<TypePrinters, anyhow::Error> {
    // Namespace tracker
    let mut _namespace_tracker: Vec<String> = Vec::new();

    // Where printers are stored
    let mut printers: HashMap<String, Type> = HashMap::new();

    let debug_info = DebugInfo::from_raw(elf).unwrap();
    let mut units = debug_info.get_units();
    'outer: while let Some(unit_info) = debug_info.get_next_unit_info(&mut units) {
        let mut entries = unit_info.unit.entries();
        while let Some(die_cursor_state) = &mut unit_info.get_next_namespace_die(&mut entries) {
            let types = unit_info.get_types(&mut die_cursor_state.clone()).unwrap();
            printers.extend(types.into_iter().map(|t| (t.name().to_string(), t)));
            // Early abort for less bloat.
            if die_cursor_state.name == "mod2" {
                break 'outer;
            }
        }
    }

    println!("Printers: {:#?}", printers);

    Ok(TypePrinters(printers))
}

type R = gimli::EndianReader<gimli::LittleEndian, std::rc::Rc<[u8]>>;
type DwarfReader = gimli::read::EndianRcSlice<gimli::LittleEndian>;
pub struct DebugInfo {
    dwarf: gimli::Dwarf<DwarfReader>,
    _frame_section: gimli::DebugFrame<DwarfReader>,
}

impl DebugInfo {
    // Parse debug information directly from a buffer containing an ELF file.
    pub fn from_raw(data: &[u8]) -> Result<Self, ()> {
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

    fn get_units(&self) -> UnitIter {
        self.dwarf.units()
    }

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

type UnitIter =
    gimli::DebugInfoUnitHeadersIter<gimli::EndianReader<gimli::LittleEndian, std::rc::Rc<[u8]>>>;

fn extract_name(
    debug_info: &DebugInfo,
    attribute_value: gimli::AttributeValue<R>,
) -> Option<String> {
    match attribute_value {
        gimli::AttributeValue::DebugStrRef(name_ref) => {
            let name_raw = debug_info.dwarf.string(name_ref).unwrap();
            Some(String::from_utf8_lossy(&name_raw).to_string())
        }
        gimli::AttributeValue::String(name) => Some(String::from_utf8_lossy(&name).to_string()),
        _ => None,
    }
}

struct UnitInfo<'debuginfo> {
    debug_info: &'debuginfo DebugInfo,
    unit: gimli::Unit<gimli::EndianReader<gimli::LittleEndian, std::rc::Rc<[u8]>>, usize>,
}

impl<'debuginfo, 'abbrev, 'unit> UnitInfo<'debuginfo> {
    fn get_next_namespace_die(
        &self,
        entries_cursor: &mut EntriesCursor<'abbrev, 'unit>,
    ) -> Option<DieCursorState<'abbrev, 'unit>> {
        while let Ok(Some((depth, current))) = entries_cursor.next_dfs() {
            match current.tag() {
                gimli::DW_TAG_namespace => {
                    let mut name = String::new();
                    let mut attrs = current.attrs();
                    while let Ok(Some(attr)) = attrs.next() {
                        match attr.name() {
                            gimli::DW_AT_name => {
                                name = extract_name(&self.debug_info, attr.value())
                                    .unwrap_or_else(|| "<undefined>".to_string());
                            }
                            _ => (),
                        }
                    }
                    return Some(DieCursorState {
                        _depth: depth,
                        name: name,
                        namespace_die: current.clone(),
                        entries_cursor: entries_cursor.clone(),
                    });
                }
                _ => (),
            };
        }
        None
    }
}

fn extract_type(unit_info: &UnitInfo, node: EntriesTreeNode<R>) -> Option<Type> {
    // Examine the entry attributes.
    let entry = node.entry();
    match entry.tag() {
        gimli::DW_TAG_structure_type => {
            let type_name = extract_name(
                &unit_info.debug_info,
                entry.attr(gimli::DW_AT_name).unwrap().unwrap().value(),
            );
            if !node.entry().has_children() {
                return Some(Type::PlainVariant(
                    type_name.unwrap_or_else(|| "<unnamed type>".to_string()),
                ));
            }
            let mut named_children = std::collections::HashMap::new();
            let mut indexed_children = Vec::new();
            let mut variants = std::collections::HashMap::new();

            let mut children = node.children();
            while let Ok(Some(child)) = children.next() {
                let entry = child.entry();
                match entry.tag() {
                    gimli::DW_TAG_member => {
                        let (name, typ) = extract_member(unit_info, child);
                        if name.starts_with("__") {
                            indexed_children.insert(
                                name.strip_prefix("__").unwrap().parse().unwrap(),
                                typ.unwrap_or(Type::Unknown),
                            );
                        } else {
                            named_children.insert(name, typ.unwrap_or(Type::Unknown));
                        }
                    }
                    gimli::DW_TAG_variant_part => {
                        while let Ok(Some(child)) = children.next() {
                            let entry = child.entry();
                            if entry.tag() == gimli::DW_TAG_structure_type {
                                let mut name = String::new();
                                let mut attrs = entry.attrs();
                                while let Ok(Some(attr)) = attrs.next() {
                                    match attr.name() {
                                        gimli::DW_AT_name => {
                                            name = extract_name(unit_info.debug_info, attr.value())
                                                .unwrap_or_else(|| "<undefined>".to_string());
                                        }
                                        _ => (),
                                    }
                                }
                                variants.insert(
                                    name,
                                    extract_type(unit_info, child).unwrap_or(Type::Unknown),
                                );
                            } else {
                                break;
                            }
                        }
                    }
                    _tag => {}
                };
            }

            if !named_children.is_empty() {
                return Some(Type::Struct(Struct {
                    name: type_name.unwrap_or_else(|| "<unnamed type>".to_string()),
                    named_children,
                    indexed_children,
                }));
            } else if !indexed_children.is_empty() {
                return Some(Type::Struct(Struct {
                    name: type_name.unwrap_or_else(|| "<unnamed type>".to_string()),
                    named_children,
                    indexed_children,
                }));
            } else if !variants.is_empty() {
                return Some(Type::Enum(Enum {
                    name: type_name.unwrap_or_else(|| "<unnamed type>".to_string()),
                    variants,
                }));
            }
        }
        gimli::DW_TAG_base_type => {
            if let Ok(Some((name, enc, size))) =
                get_base_type_info(&unit_info.debug_info.dwarf, &entry)
            {
                return Some(elf_test::Type::new_from_base_type(enc, &name, size));
            }
        }
        t => println!("Unknown type class: {}", t),
    };

    return None;
}

fn extract_member(unit_info: &UnitInfo, node: EntriesTreeNode<R>) -> (String, Option<Type>) {
    let mut name = "".into();
    let mut typ = None;
    let mut attrs = node.entry().attrs();
    while let Ok(Some(attr)) = attrs.next() {
        match attr.name() {
            gimli::DW_AT_name => {
                name = extract_name(&unit_info.debug_info, attr.value())
                    .unwrap_or_else(|| "<undefined>".to_string());
            }
            gimli::DW_AT_type => {
                let mut tree = unit_info
                    .unit
                    .entries_tree(Some(match attr.value() {
                        AttributeValue::UnitRef(v) => v,
                        _ => panic!(),
                    }))
                    .unwrap();
                let root = tree.root().unwrap();
                typ = extract_type(unit_info, root);
            }
            _attr => (),
        }
    }

    return (name, typ);
}

#[derive(Debug)]
pub struct Variable {
    pub name: String,
    pub file: String,
    pub line: u64,
    pub value: u64,
    pub typ: Type,
}

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

#[derive(Clone)]
struct DieCursorState<'abbrev, 'unit> {
    entries_cursor: EntriesCursor<'abbrev, 'unit>,
    _depth: isize,
    name: String,
    namespace_die: NamespaceDie<'abbrev, 'unit>,
}

impl<'debuginfo> UnitInfo<'debuginfo> {
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
                    typ: Type::Unknown,
                };
                let mut attrs = current.attrs();
                while let Ok(Some(attr)) = attrs.next() {
                    match attr.name() {
                        gimli::DW_AT_name => {
                            variable.name = extract_name(&self.debug_info, attr.value())
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

    fn get_types(&self, die_cursor_state: &mut DieCursorState) -> Result<Vec<Type>, ()> {
        let mut types = vec![];

        let mut tree = self
            .unit
            .entries_tree(Some(die_cursor_state.namespace_die.offset()))
            .unwrap();
        let namespace = tree.root().unwrap();
        let mut children = namespace.children();

        while let Ok(Some(current)) = children.next() {
            if let gimli::DW_TAG_structure_type = current.entry().tag() {
                if let Some(typ) = extract_type(self, current) {
                    types.push(typ);
                }
            };
        }

        Ok(types)
    }
}

fn main() -> Result<(), anyhow::Error> {
    let opts = Opts::from_args();
    println!("opts: {:#?}", opts.elf);

    let bytes = fs::read(opts.elf)?;

    let _printers = generate_printers(&bytes)?;

    Ok(())
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
        if attr.name() == constants::DW_AT_name {
            if let AttributeValue::DebugStrRef(r) = attr.value() {
                if let Ok(s) = dwarf.string(r) {
                    if let Ok(s) = s.to_string() {
                        name = Some(s.into());
                    }
                }
            }
        }

        // Find encoding
        if attr.name() == constants::DW_AT_encoding {
            if let AttributeValue::Encoding(enc) = attr.value() {
                encoding = Some(enc);
            }
        }

        // Find size
        if attr.name() == constants::DW_AT_byte_size {
            if let AttributeValue::Udata(s) = attr.value() {
                size = Some(s.try_into()?);
            }
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
