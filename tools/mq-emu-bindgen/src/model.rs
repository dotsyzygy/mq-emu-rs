//! Builds a layout model of selected classes from a PDB.
//!
//! Records are tracked by PDB type index, not by name: anonymous unions and
//! structs all share names like `<unnamed-tag>`, and forward references must
//! be resolved to the one complete definition.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fmt;

use pdb2::{FallibleIterator, PrimitiveKind, TypeData, TypeFinder, TypeIndex, Variant};

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<pdb2::Error> for Error {
    fn from(e: pdb2::Error) -> Self {
        Error(format!("pdb: {e}"))
    }
}

type Result<T> = std::result::Result<T, Error>;

/// Identifies a type by its PDB type index.
pub type TypeId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prim {
    Bool,
    CChar,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
}

impl Prim {
    pub fn size(self) -> u64 {
        match self {
            Prim::Bool | Prim::CChar | Prim::I8 | Prim::U8 => 1,
            Prim::I16 | Prim::U16 => 2,
            Prim::I32 | Prim::U32 | Prim::F32 => 4,
            Prim::I64 | Prim::U64 | Prim::F64 => 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Prim(Prim),
    /// `void`; only appears behind a pointer.
    Void,
    /// A function type; only appears behind a pointer.
    Function,
    Pointer(Box<Ty>),
    Array(Box<Ty>, u64),
    /// A class, struct or union. Generated in full when reached by value,
    /// otherwise emitted as an opaque type.
    Record(TypeId),
    Enum(TypeId),
    /// Something with a known size but no useful structure (e.g. pointer to member).
    Bytes(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bits {
    pub position: u8,
    pub length: u8,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    /// Class whose definition declares this member (a base class for inherited members).
    pub declared_in: String,
    /// Offset from the start of the record.
    pub offset: u64,
    /// Offset from the start of `declared_in`, as written in eqlib's offset comments.
    pub decl_offset: u64,
    pub ty: Ty,
    pub bits: Option<Bits>,
}

#[derive(Debug, Clone)]
pub struct Record {
    pub cpp_name: String,
    pub size: u64,
    pub is_union: bool,
    /// Sorted by offset, then bit position.
    pub fields: Vec<Field>,
    /// For anonymous types: the record and member that first contained it.
    pub parent_hint: Option<(TypeId, String)>,
}

#[derive(Debug, Clone)]
pub struct Enum {
    pub cpp_name: String,
    pub underlying: Prim,
    pub values: Vec<(String, i128)>,
}

#[derive(Debug, Default)]
pub struct Model {
    /// Records generated in full (reached by value from a root).
    pub records: BTreeMap<TypeId, Record>,
    pub enums: BTreeMap<TypeId, Enum>,
    /// Records only reached behind pointers, with their C++ names.
    pub opaque: BTreeMap<TypeId, String>,
    /// The roots, in the order requested.
    pub roots: Vec<TypeId>,
}

impl Model {
    pub fn size_of(&self, ty: &Ty) -> u64 {
        match ty {
            Ty::Prim(p) => p.size(),
            Ty::Pointer(_) => 4,
            Ty::Array(_, bytes) => *bytes,
            Ty::Record(id) => self.records.get(id).map_or(0, |r| r.size),
            Ty::Enum(id) => self.enums[id].underlying.size(),
            Ty::Bytes(n) => *n,
            Ty::Void | Ty::Function => 0,
        }
    }
}

/// Strips the namespaces every eqlib type lives in, so names match the headers.
pub fn display_name(raw: &str) -> String {
    raw.replace("eqlib::", "")
}

pub fn is_anonymous(name: &str) -> bool {
    name.contains("<unnamed") || name.contains("<anonymous")
}

/// Loads `pdb_path` and builds the model for `roots` (C++ class names without
/// the `eqlib::` namespace) and every record they contain by value.
pub fn build(pdb_path: &str, roots: &[&str]) -> Result<Model> {
    let file = std::fs::File::open(pdb_path).map_err(|e| Error(format!("{pdb_path}: {e}")))?;
    let mut pdb = pdb2::PDB::open(file)?;
    let info = pdb.type_information()?;
    let mut finder = info.finder();

    // Index complete definitions so forward references can be resolved.
    let mut by_unique: HashMap<String, TypeIndex> = HashMap::new();
    let mut by_name: HashMap<String, TypeIndex> = HashMap::new();
    let mut definitions: HashMap<String, Vec<TypeIndex>> = HashMap::new();
    let mut iter = info.iter();
    while let Some(item) = iter.next()? {
        finder.update(&iter);
        let (name, unique, forward) = match item.parse() {
            Ok(TypeData::Class(c)) => (c.name.to_string(), c.unique_name.map(|u| u.to_string()), c.properties.forward_reference()),
            Ok(TypeData::Union(u)) => (u.name.to_string(), u.unique_name.map(|u| u.to_string()), u.properties.forward_reference()),
            Ok(TypeData::Enumeration(e)) => (e.name.to_string(), e.unique_name.map(|u| u.to_string()), e.properties.forward_reference()),
            _ => continue,
        };
        if forward {
            continue;
        }
        if let Some(unique) = unique {
            by_unique.entry(unique.into_owned()).or_insert(item.index());
        }
        if !is_anonymous(&name) {
            let name = display_name(&name);
            by_name.entry(name.clone()).or_insert(item.index());
            definitions.entry(name).or_default().push(item.index());
        }
    }

    // The linker merges identical type records, so two complete definitions
    // with one name normally means two *different* layouts, e.g. a stale one
    // from an old object file or compiler PDB. Using either would be a guess.
    let mut conflicts = HashMap::new();
    for (name, indexes) in definitions.into_iter().filter(|(_, indexes)| indexes.len() > 1) {
        let signatures = indexes.iter().map(|index| signature(&finder, *index)).collect::<Result<Vec<_>>>()?;
        if differ(&signatures) {
            conflicts.insert(name, indexes);
        }
    }

    let mut builder =
        Builder { finder: &finder, by_unique, by_name, conflicts, model: Model::default(), queue: VecDeque::new() };

    for root in roots {
        let index = *builder.by_name.get(*root).ok_or_else(|| Error(format!("type `{root}` not found in {pdb_path}")))?;
        match builder.parse(index)? {
            TypeData::Class(_) | TypeData::Union(_) => {}
            other => return Err(Error(format!("`{root}` is not a class, struct or union: {other:?}"))),
        }
        builder.model.roots.push(index.0);
        builder.queue.push_back((index, None));
    }

    while let Some((index, parent_hint)) = builder.queue.pop_front() {
        if builder.model.records.contains_key(&index.0) {
            continue;
        }
        let record = builder.record(index, parent_hint)?;
        builder.model.records.insert(index.0, record);
    }

    // Anything reached behind a pointer that also ended up generated isn't opaque.
    let Builder { mut model, .. } = builder;
    let generated: BTreeSet<TypeId> = model.records.keys().copied().collect();
    model.opaque.retain(|id, _| !generated.contains(id));
    Ok(model)
}

struct Builder<'a, 't> {
    finder: &'a TypeFinder<'t>,
    by_unique: HashMap<String, TypeIndex>,
    by_name: HashMap<String, TypeIndex>,
    /// Names with several different complete definitions in the PDB.
    conflicts: HashMap<String, Vec<TypeIndex>>,
    model: Model,
    queue: VecDeque<(TypeIndex, Option<(TypeId, String)>)>,
}

impl<'t> Builder<'_, 't> {
    fn parse(&self, index: TypeIndex) -> Result<TypeData<'t>> {
        Ok(self.finder.find(index)?.parse()?)
    }

    /// Refuses to lay out a type whose name has conflicting definitions.
    fn check_unambiguous(&self, name: &str) -> Result<()> {
        match self.conflicts.get(name) {
            Some(indexes) => Err(Error(format!(
                "the PDB has {} different definitions of `{name}` (type indexes {indexes:?}), so its layout is ambiguous. \
                 The linker updates an existing PDB in place and never drops old types, so this usually means                  eqlib.pdb has survived from an older build: delete build/bin/<config>/eqlib.pdb (and eqlib.dll, to force a                  relink), rebuild MacroQuest, and regenerate.",
                indexes.len()
            ))),
            None => Ok(()),
        }
    }

    /// Maps a forward reference to its complete definition, if the PDB has one.
    fn resolve(&self, index: TypeIndex) -> Result<TypeIndex> {
        let (name, unique, forward) = match self.parse(index)? {
            TypeData::Class(c) => (c.name.to_string().into_owned(), c.unique_name.map(|u| u.to_string().into_owned()), c.properties.forward_reference()),
            TypeData::Union(u) => (u.name.to_string().into_owned(), u.unique_name.map(|u| u.to_string().into_owned()), u.properties.forward_reference()),
            TypeData::Enumeration(e) => (e.name.to_string().into_owned(), e.unique_name.map(|u| u.to_string().into_owned()), e.properties.forward_reference()),
            _ => return Ok(index),
        };
        if !forward {
            return Ok(index);
        }
        let found = unique
            .and_then(|u| self.by_unique.get(&u).copied())
            .or_else(|| (!is_anonymous(&name)).then(|| self.by_name.get(&display_name(&name)).copied()).flatten());
        Ok(found.unwrap_or(index))
    }

    fn record(&mut self, index: TypeIndex, parent_hint: Option<(TypeId, String)>) -> Result<Record> {
        let (cpp_name, size, is_union, fields) = match self.parse(index)? {
            TypeData::Class(c) => (display_name(&c.name.to_string()), c.size, false, c.fields),
            TypeData::Union(u) => (display_name(&u.name.to_string()), u.size, true, Some(u.fields)),
            other => return Err(Error(format!("type {index:?} is not a record: {other:?}"))),
        };
        self.check_unambiguous(&cpp_name)?;

        let mut out = Vec::new();
        if let Some(fields) = fields {
            self.collect_fields(index.0, fields, 0, &cpp_name, &mut out)?;
            self.collect_virtual_bases(index.0, fields, size, &mut out)?;
        }
        out.sort_by_key(|f| (f.offset, f.bits.map_or(0, |b| b.position)));
        // A vbptr shared through several bases is reported once per base.
        out.dedup_by(|a, b| a.name == b.name && a.offset == b.offset && a.name == "__vbptr");
        Ok(Record { cpp_name, size, is_union, fields: out, parent_hint })
    }

    /// Lays out virtual bases. MSVC places them after all non-virtual parts of the
    /// most-derived class, in vbtable order, so they fill the record from the end.
    /// The most-derived class lists every virtual base, direct and indirect.
    fn collect_virtual_bases(&mut self, owner: TypeId, list: TypeIndex, size: u64, out: &mut Vec<Field>) -> Result<()> {
        let mut vbases = Vec::new();
        let mut next = Some(list);
        while let Some(list) = next {
            let TypeData::FieldList(list) = self.parse(list)? else { break };
            for field in &list.fields {
                if let TypeData::VirtualBaseClass(v) = field {
                    vbases.push((v.virtual_base_offset, self.resolve(v.base_class)?));
                }
            }
            next = list.continuation;
        }
        vbases.sort_by_key(|(order, _)| *order);

        let mut end = size;
        for (_, base) in vbases.into_iter().rev() {
            let TypeData::Class(class) = self.parse(base)? else {
                return Err(Error(format!("virtual base {base:?} is not a class")));
            };
            end = end.checked_sub(class.size).ok_or_else(|| Error(format!("virtual bases overflow record {owner}")))?;
            if let Some(fields) = class.fields {
                self.collect_fields(owner, fields, end, &display_name(&class.name.to_string()), out)?;
            }
        }
        Ok(())
    }

    /// Appends the members of a field list, flattening base classes into the record.
    fn collect_fields(&mut self, owner: TypeId, list: TypeIndex, base_offset: u64, declared_in: &str, out: &mut Vec<Field>) -> Result<()> {
        let mut next = Some(list);
        while let Some(list) = next {
            let TypeData::FieldList(list) = self.parse(list)? else {
                return Err(Error(format!("expected a field list at {list:?}")));
            };
            for field in &list.fields {
                match field {
                    TypeData::Member(m) => {
                        let name = m.name.to_string().into_owned();
                        let (ty, bits) = match self.parse(m.field_type)? {
                            TypeData::Bitfield(b) => {
                                let ty = self.ty(b.underlying_type, Some((owner, &name)))?;
                                (ty, Some(Bits { position: b.position, length: b.length }))
                            }
                            _ => (self.ty(m.field_type, Some((owner, &name)))?, None),
                        };
                        out.push(Field {
                            name,
                            declared_in: declared_in.to_owned(),
                            offset: base_offset + m.offset,
                            decl_offset: m.offset,
                            ty,
                            bits,
                        });
                    }
                    TypeData::BaseClass(b) => {
                        let base = self.resolve(b.base_class)?;
                        let TypeData::Class(class) = self.parse(base)? else {
                            return Err(Error(format!("base class {base:?} is not a class")));
                        };
                        if let Some(fields) = class.fields {
                            let base_name = display_name(&class.name.to_string());
                            self.collect_fields(owner, fields, base_offset + u64::from(b.offset), &base_name, out)?;
                        }
                    }
                    // The base itself is placed by `collect_virtual_bases`; here we only
                    // record the pointer to the vbtable that this class carries.
                    TypeData::VirtualBaseClass(v) => {
                        if v.direct {
                            out.push(Field {
                                name: "__vbptr".to_owned(),
                                declared_in: declared_in.to_owned(),
                                offset: base_offset + u64::from(v.base_pointer_offset),
                                decl_offset: u64::from(v.base_pointer_offset),
                                ty: Ty::Pointer(Box::new(Ty::Void)),
                                bits: None,
                            });
                        }
                    }
                    TypeData::VirtualFunctionTablePointer(_) => out.push(Field {
                        name: "__vfptr".to_owned(),
                        declared_in: declared_in.to_owned(),
                        offset: base_offset,
                        decl_offset: 0,
                        ty: Ty::Pointer(Box::new(Ty::Void)),
                        bits: None,
                    }),
                    // Methods, static members, nested type declarations, enumerators.
                    _ => {}
                }
            }
            next = list.continuation;
        }
        Ok(())
    }

    /// Converts a type reached by value. `context` names the member it was reached
    /// through, so anonymous records can be named after it.
    fn ty(&mut self, index: TypeIndex, context: Option<(TypeId, &str)>) -> Result<Ty> {
        Ok(match self.parse(index)? {
            TypeData::Primitive(p) => {
                let base = prim_ty(p.kind)?;
                if p.indirection.is_some() { Ty::Pointer(Box::new(base)) } else { base }
            }
            TypeData::Class(_) | TypeData::Union(_) => {
                let resolved = self.resolve(index)?;
                self.queue.push_back((resolved, context.map(|(owner, member)| (owner, member.to_owned()))));
                Ty::Record(resolved.0)
            }
            TypeData::Enumeration(_) => self.enumeration(index)?,
            TypeData::Modifier(m) => self.ty(m.underlying_type, context)?,
            TypeData::Pointer(p) => {
                if p.attributes.pointer_to_member() {
                    Ty::Bytes(u64::from(p.attributes.size()))
                } else {
                    Ty::Pointer(Box::new(self.pointee(p.underlying_type)?))
                }
            }
            TypeData::Array(a) => {
                let element = self.ty(a.element_type, context)?;
                let bytes = u64::from(*a.dimensions.last().unwrap_or(&0));
                Ty::Array(Box::new(element), bytes)
            }
            TypeData::Procedure(_) | TypeData::MemberFunction(_) => Ty::Function,
            other => return Err(Error(format!("unsupported member type {index:?}: {other:?}"))),
        })
    }

    /// Converts a type reached through a pointer: records stay opaque unless
    /// something else contains them by value.
    fn pointee(&mut self, index: TypeIndex) -> Result<Ty> {
        Ok(match self.parse(index)? {
            TypeData::Primitive(p) => {
                let base = prim_ty(p.kind)?;
                if p.indirection.is_some() { Ty::Pointer(Box::new(base)) } else { base }
            }
            TypeData::Class(_) | TypeData::Union(_) => {
                let resolved = self.resolve(index)?;
                let name = match self.parse(resolved)? {
                    TypeData::Class(c) => display_name(&c.name.to_string()),
                    TypeData::Union(u) => display_name(&u.name.to_string()),
                    _ => unreachable!(),
                };
                self.model.opaque.entry(resolved.0).or_insert(name);
                Ty::Record(resolved.0)
            }
            TypeData::Enumeration(_) => self.enumeration(index)?,
            TypeData::Modifier(m) => self.pointee(m.underlying_type)?,
            TypeData::Pointer(p) => {
                if p.attributes.pointer_to_member() {
                    Ty::Bytes(u64::from(p.attributes.size()))
                } else {
                    Ty::Pointer(Box::new(self.pointee(p.underlying_type)?))
                }
            }
            TypeData::Array(a) => {
                let element = self.pointee(a.element_type)?;
                Ty::Array(Box::new(element), u64::from(*a.dimensions.last().unwrap_or(&0)))
            }
            TypeData::Procedure(_) | TypeData::MemberFunction(_) => Ty::Function,
            other => return Err(Error(format!("unsupported pointee type {index:?}: {other:?}"))),
        })
    }

    fn enumeration(&mut self, index: TypeIndex) -> Result<Ty> {
        let index = self.resolve(index)?;
        if self.model.enums.contains_key(&index.0) {
            return Ok(Ty::Enum(index.0));
        }
        let TypeData::Enumeration(e) = self.parse(index)? else { unreachable!() };
        self.check_unambiguous(&display_name(&e.name.to_string()))?;
        let underlying = match self.parse(e.underlying_type)? {
            TypeData::Primitive(p) => match prim_ty(p.kind)? {
                Ty::Prim(p) => p,
                other => return Err(Error(format!("enum with non-integer underlying type {other:?}"))),
            },
            other => return Err(Error(format!("enum with unexpected underlying type {other:?}"))),
        };

        let mut values = Vec::new();
        if !e.properties.forward_reference() {
            let mut next = Some(e.fields);
            while let Some(list) = next {
                let TypeData::FieldList(list) = self.parse(list)? else { break };
                for field in &list.fields {
                    if let TypeData::Enumerate(v) = field {
                        values.push((v.name.to_string().into_owned(), variant_value(v.value)));
                    }
                }
                next = list.continuation;
            }
        }
        self.model.enums.insert(index.0, Enum { cpp_name: display_name(&e.name.to_string()), underlying, values });
        Ok(Ty::Enum(index.0))
    }
}

/// What distinguishes one definition of a type from another: its size and its
/// members' names and offsets (or an enum's values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Signature {
    size: u64,
    members: Vec<(String, u64)>,
}

fn signature(finder: &TypeFinder<'_>, index: TypeIndex) -> Result<Signature> {
    let (size, fields) = match finder.find(index)?.parse()? {
        TypeData::Class(c) => (c.size, c.fields),
        TypeData::Union(u) => (u.size, Some(u.fields)),
        TypeData::Enumeration(e) => (0, Some(e.fields)),
        _ => return Ok(Signature { size: 0, members: Vec::new() }),
    };
    let mut members = Vec::new();
    let mut next = fields;
    while let Some(list) = next {
        let TypeData::FieldList(list) = finder.find(list)?.parse()? else { break };
        for field in &list.fields {
            match field {
                TypeData::Member(m) => members.push((m.name.to_string().into_owned(), m.offset)),
                TypeData::BaseClass(b) => members.push((format!("<base {}>", b.base_class.0), u64::from(b.offset))),
                TypeData::Enumerate(v) => members.push((v.name.to_string().into_owned(), variant_value(v.value) as u64)),
                _ => {}
            }
        }
        next = list.continuation;
    }
    Ok(Signature { size, members })
}

/// True if any two signatures differ.
pub(crate) fn differ(signatures: &[Signature]) -> bool {
    signatures.windows(2).any(|pair| pair[0] != pair[1])
}

fn prim_ty(kind: PrimitiveKind) -> Result<Ty> {
    Ok(match kind {
        PrimitiveKind::Void | PrimitiveKind::NoType => Ty::Void,
        PrimitiveKind::Bool8 => Ty::Prim(Prim::Bool),
        PrimitiveKind::Char | PrimitiveKind::RChar => Ty::Prim(Prim::CChar),
        PrimitiveKind::I8 => Ty::Prim(Prim::I8),
        PrimitiveKind::UChar | PrimitiveKind::U8 | PrimitiveKind::Char8 => Ty::Prim(Prim::U8),
        PrimitiveKind::Short | PrimitiveKind::I16 => Ty::Prim(Prim::I16),
        PrimitiveKind::UShort | PrimitiveKind::U16 | PrimitiveKind::WChar | PrimitiveKind::RChar16 => Ty::Prim(Prim::U16),
        PrimitiveKind::Long | PrimitiveKind::I32 | PrimitiveKind::HRESULT => Ty::Prim(Prim::I32),
        PrimitiveKind::ULong | PrimitiveKind::U32 | PrimitiveKind::RChar32 | PrimitiveKind::Bool32 => Ty::Prim(Prim::U32),
        PrimitiveKind::Quad | PrimitiveKind::I64 => Ty::Prim(Prim::I64),
        PrimitiveKind::UQuad | PrimitiveKind::U64 => Ty::Prim(Prim::U64),
        PrimitiveKind::F32 => Ty::Prim(Prim::F32),
        PrimitiveKind::F64 => Ty::Prim(Prim::F64),
        other => return Err(Error(format!("unsupported primitive {other:?}"))),
    })
}

fn variant_value(v: Variant) -> i128 {
    match v {
        Variant::U8(v) => v.into(),
        Variant::U16(v) => v.into(),
        Variant::U32(v) => v.into(),
        Variant::U64(v) => v.into(),
        Variant::I8(v) => v.into(),
        Variant::I16(v) => v.into(),
        Variant::I32(v) => v.into(),
        Variant::I64(v) => v.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(size: u64, members: &[(&str, u64)]) -> Signature {
        Signature { size, members: members.iter().map(|(n, o)| (n.to_string(), *o)).collect() }
    }

    #[test]
    fn identical_definitions_do_not_conflict() {
        let a = sig(8, &[("x", 0), ("y", 4)]);
        assert!(!differ(&[a.clone(), a.clone(), a]));
        assert!(!differ(&[sig(8, &[])]));
        assert!(!differ(&[]));
    }

    #[test]
    fn differing_definitions_conflict() {
        // The case found in practice: same size, a member renamed and moved.
        let stale = sig(0x83f9c, &[("WorldServerShortname", 0x367f8), ("combatSkill", 0x36838), ("temp", 0x5d34e)]);
        let current = sig(0x83f9c, &[("WorldServerShortname", 0x367f8), ("combatSkill", 0x36820), ("bHotButtonChanged", 0x5d336)]);
        assert!(differ(&[stale.clone(), current.clone()]));
        assert!(differ(&[current.clone(), current, stale]));
        assert!(differ(&[sig(4, &[("x", 0)]), sig(8, &[("x", 0)])]));
    }
}
