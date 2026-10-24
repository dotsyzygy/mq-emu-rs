//! Game-object layouts generated from `eqlib.pdb` by `tools/mq-emu-bindgen`.
//!
//! Each C++ record is an opaque byte block with accessor methods named after
//! the C++ members (`X()`, `set_Heading()`), including members inherited from
//! base classes. Records are only ever reached through pointers into game
//! memory: turning such a pointer into a reference is `unsafe`, after which
//! the accessors are safe. All access must happen on the game thread; the
//! types are `!Sync`, so references can't be shared with other threads.
//!
//! The roots are listed in `layouts.txt`; records contained by value are
//! generated in full, and types only reached through pointers are opaque.

#[rustfmt::skip]
#[allow(non_snake_case, non_camel_case_types, non_upper_case_globals, clippy::all)]
mod generated;

pub use generated::*;

/// Describes one generated record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordInfo {
    /// C++ name, without the `eqlib::` namespace.
    pub name: &'static str,
    pub rust_name: &'static str,
    pub size: u32,
}

/// Describes one generated field accessor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldInfo {
    /// C++ name of the record the accessor is on.
    pub record: &'static str,
    /// C++ member name.
    pub field: &'static str,
    /// Getter name (the setter is `set_<accessor>`).
    pub accessor: &'static str,
    /// Class that declares the member; a base class for inherited members.
    pub declared_in: &'static str,
    /// Offset within `record`.
    pub offset: u32,
    /// Offset within `declared_in`, as eqlib's offset comments count it.
    pub decl_offset: u32,
    pub size: u32,
    /// `(position, length)` for bitfields.
    pub bits: Option<(u8, u8)>,
}
