use elf_test::{Enum, Struct, Type, TypeKind};
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
        let types = unit_info.list_types().unwrap();
        printers.extend(types.into_iter().map(|t| (t.name().to_string(), t)));
        // while let Some(die_cursor_state) = &mut unit_info.get_next_namespace_die(&mut entries) {
        //     let types = unit_info.get_types(&mut die_cursor_state.clone()).unwrap();
        //     printers.extend(types.into_iter().map(|t| (t.name().to_string(), t)));
        //     // Early abort for less bloat.
        //     if die_cursor_state.name == "mod2" {
        //         break 'outer;
        //     }
        // }
    }

    println!("Printers: {:#?}", printers);

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
        let mut root = tree.root().unwrap();
        self.walk_namespace(root, vec![])
    }

    fn walk_namespace(
        &self,
        node: EntriesTreeNode<R>,
        mut current_namespace: Vec<String>,
    ) -> Result<Vec<Type>, ()> {
        let mut tree = self.unit.entries_tree(Some(node.entry().offset())).unwrap();
        let root = tree.root().unwrap();
        let namespace =
            self.extract_string_of(&root.entry().attr(gimli::DW_AT_name).unwrap().unwrap());
        // Filter namespaces for less output bloat.
        let mut types = if namespace == Some("mod2".into()) {
            self.get_types(root, current_namespace.clone()).unwrap()
        } else {
            vec![]
        };
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

    /// Returns the next DIE that marks a namespace in the `entries_cursor`.
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
                                name = self
                                    .extract_string_of(&attr)
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

    /// Returns the type that `node` represents.
    fn extract_type_of(
        &self,
        node: EntriesTreeNode<R>,
        current_namespace: Vec<String>,
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
                            let (name, typ) =
                                self.extract_member_of(child, current_namespace.clone());
                            if name.starts_with("__") {
                                let index = name.strip_prefix("__").unwrap().parse().unwrap();
                                indexed_children.insert(
                                    index,
                                    typ.unwrap_or(Type::new(
                                        TypeKind::Unknown,
                                        index.to_string(),
                                        current_namespace.clone(),
                                    )),
                                );
                            } else {
                                named_children.insert(
                                    name.clone(),
                                    typ.unwrap_or(Type::new(
                                        TypeKind::Unknown,
                                        name,
                                        current_namespace.clone(),
                                    )),
                                );
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
                                                name = self
                                                    .extract_string_of(&attr)
                                                    .unwrap_or_else(|| "<undefined>".to_string());
                                            }
                                            _ => (),
                                        }
                                    }
                                    variants.insert(
                                        name.clone(),
                                        self.extract_type_of(child, current_namespace.clone())
                                            .unwrap_or(Type::new(
                                                TypeKind::Unknown,
                                                name,
                                                current_namespace.clone(),
                                            )),
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
                    return Some(Type::new(
                        TypeKind::Struct(Struct {
                            named_children,
                            indexed_children,
                        }),
                        type_name.unwrap_or_else(|| "<unnamed type>".to_string()),
                        current_namespace,
                    ));
                } else if !indexed_children.is_empty() {
                    return Some(Type::new(
                        TypeKind::Struct(Struct {
                            named_children,
                            indexed_children,
                        }),
                        type_name.unwrap_or_else(|| "<unnamed type>".to_string()),
                        current_namespace,
                    ));
                } else if !variants.is_empty() {
                    return Some(Type::new(
                        TypeKind::Enum(Enum { variants }),
                        type_name.unwrap_or_else(|| "<unnamed type>".to_string()),
                        current_namespace,
                    ));
                }
            }
            gimli::DW_TAG_base_type => {
                if let Ok(Some((name, enc, size))) =
                    get_base_type_info(&self.debug_info.dwarf, &entry)
                {
                    return Some(Type::new(
                        elf_test::TypeKind::new_from_base_type(enc, &name, size),
                        name,
                        current_namespace,
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
    ) -> (String, Option<Type>) {
        let mut name = "".into();
        let mut typ = None;
        let mut attrs = node.entry().attrs();
        while let Ok(Some(attr)) = attrs.next() {
            match attr.name() {
                gimli::DW_AT_name => {
                    name = self
                        .extract_string_of(&attr)
                        .unwrap_or_else(|| "<undefined>".to_string());
                }
                gimli::DW_AT_type => {
                    let mut tree = self
                        .unit
                        .entries_tree(Some(match attr.value() {
                            AttributeValue::UnitRef(v) => v,
                            _ => panic!(),
                        }))
                        .unwrap();
                    let root = tree.root().unwrap();
                    typ = self.extract_type_of(root, current_namespace.clone());
                }
                _attr => (),
            }
        }

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
                    typ: Type::new(TypeKind::Unknown, String::new(), vec![]),
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
                if let Some(typ) = self.extract_type_of(current, current_namespace.clone()) {
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
