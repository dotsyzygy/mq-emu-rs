//! Exercises generated accessors against fake objects: each test writes through
//! an accessor and checks the raw bytes at the offset recorded in `FIELDS` (or
//! the reverse), so a wrong offset, width or bit mask in the emitter fails here.

use core::ffi::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};

use mq_emu_sys::layouts::{EActorType, FIELDS, PZCPhysicsInfo, PcClient, PlayerClient, RECORDS};

/// Zeroed, 8-byte-aligned storage the size of `record`, viewed as `T`.
struct Fake {
    storage: Vec<u64>,
}

impl Fake {
    fn new(record: &str) -> Fake {
        let size = RECORDS.iter().find(|r| r.name == record).unwrap_or_else(|| panic!("no record {record}")).size;
        Fake { storage: vec![0; (size as usize).div_ceil(8)] }
    }

    fn base(&mut self) -> *mut u8 {
        self.storage.as_mut_ptr().cast()
    }

    fn view<T>(&mut self) -> &T {
        unsafe { &*self.base().cast::<T>() }
    }

    fn bytes(&mut self, offset: u32, len: usize) -> Vec<u8> {
        unsafe { std::slice::from_raw_parts(self.base().add(offset as usize), len).to_vec() }
    }

    fn poke(&mut self, offset: u32, bytes: &[u8]) {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.base().add(offset as usize), bytes.len()) }
    }
}

fn offset(record: &str, field: &str) -> u32 {
    FIELDS
        .iter()
        .find(|f| f.record == record && f.field == field)
        .unwrap_or_else(|| panic!("no field {record}::{field}"))
        .offset
}

#[test]
fn float_round_trips_at_pdb_offset() {
    let mut fake = Fake::new("PlayerClient");
    let at = offset("PlayerClient", "X");
    fake.view::<PlayerClient>().set_X(1.5);
    assert_eq!(fake.bytes(at, 4), 1.5f32.to_le_bytes());

    fake.poke(at, &(-42.25f32).to_le_bytes());
    assert_eq!(fake.view::<PlayerClient>().X(), -42.25);
}

#[test]
fn char_arrays_copy_in_and_out() {
    let mut fake = Fake::new("PlayerClient");
    let mut name = [0 as c_char; 64];
    for (dst, src) in name.iter_mut().zip(b"Soandso") {
        *dst = *src as c_char;
    }
    fake.view::<PlayerClient>().set_Name(name);
    assert_eq!(&fake.bytes(offset("PlayerClient", "Name"), 8), b"Soandso\0");
    assert_eq!(fake.view::<PlayerClient>().Name(), name);
}

#[test]
fn pointers_round_trip() {
    let mut fake = Fake::new("PlayerClient");
    let next = 0x1234_5678usize as *mut PlayerClient;
    fake.view::<PlayerClient>().set_m_pNext(next);
    assert_eq!(fake.bytes(offset("PlayerClient", "m_pNext"), 4), 0x1234_5678u32.to_le_bytes());
    assert_eq!(fake.view::<PlayerClient>().m_pNext(), next);
}

#[test]
fn bool_treats_any_nonzero_byte_as_true() {
    let mut fake = Fake::new("PcClient");
    let at = offset("PcClient", "InCombat");
    // Game memory may hold values other than 0/1; reading must not be UB or false.
    fake.poke(at, &[2]);
    assert!(fake.view::<PcClient>().InCombat());

    fake.view::<PcClient>().set_InCombat(false);
    assert_eq!(fake.bytes(at, 1), [0]);
    fake.view::<PcClient>().set_InCombat(true);
    assert_eq!(fake.bytes(at, 1), [1]);
}

#[test]
fn bitfields_only_touch_their_own_bits() {
    // PZCPhysicsInfo packs Heading:12, DeltaHeading:10, Acceleration:10 into one u32.
    let mut fake = Fake::new("PZCPhysicsInfo");
    let at = offset("PZCPhysicsInfo", "Heading");
    fake.poke(at, &u32::MAX.to_le_bytes());
    let physics = fake.view::<PZCPhysicsInfo>();

    physics.set_Heading(0);
    assert_eq!(physics.Heading(), 0);
    assert_eq!(physics.DeltaHeading(), 0x3ff);
    assert_eq!(physics.Acceleration(), 0x3ff);

    // Values wider than the field are truncated to it, not spilled into neighbours.
    physics.set_DeltaHeading(0x1234);
    assert_eq!(physics.DeltaHeading(), 0x234);
    assert_eq!(physics.Acceleration(), 0x3ff);
    assert_eq!(fake.bytes(at, 4), 0xFFE3_4000u32.to_le_bytes());
}

#[test]
fn nested_records_point_into_their_parent() {
    let mut fake = Fake::new("PlayerClient");
    let base = fake.base() as usize;
    let casting = fake.view::<PlayerClient>().CastingData() as *const _ as usize;
    assert_eq!(casting - base, offset("PlayerClient", "CastingData") as usize);
}

#[test]
fn record_arrays_index_within_bounds() {
    let mut fake = Fake::new("PcClient");
    let base = fake.base() as usize;
    let first = fake.view::<PcClient>().Tasks(0) as *const _ as usize;
    assert_eq!(first - base, offset("PcClient", "Tasks") as usize);

    let element = RECORDS.iter().find(|r| r.name == "PCTaskStatus").unwrap().size as usize;
    let count = FIELDS.iter().find(|f| f.record == "PcClient" && f.field == "Tasks").unwrap().size as usize / element;
    let past_end = catch_unwind(AssertUnwindSafe(|| {
        fake.view::<PcClient>().Tasks(count);
    }));
    assert!(past_end.is_err(), "Tasks({count}) should be out of bounds");
}

#[test]
fn every_field_lies_inside_its_record() {
    for field in FIELDS {
        let record = RECORDS.iter().find(|r| r.name == field.record).unwrap();
        assert!(
            field.offset + field.size <= record.size,
            "{}::{} at {:#x}+{:#x} overruns {:#x}",
            field.record,
            field.field,
            field.offset,
            field.size,
            record.size
        );
        if let Some((position, length)) = field.bits {
            assert!(u32::from(position) + u32::from(length) <= field.size * 8, "{}::{} bits", field.record, field.field);
        }
    }
}

#[test]
fn records_match_rust_sizes() {
    assert_eq!(size_of::<PlayerClient>(), RECORDS.iter().find(|r| r.name == "PlayerClient").unwrap().size as usize);
    assert_eq!(size_of::<PcClient>(), 0x5804);
}

#[test]
fn enums_are_open_newtypes() {
    assert_eq!(EActorType::Player, EActorType(1));
    // Values the C++ enum doesn't list are still representable.
    assert_ne!(EActorType(9999), EActorType::Player);
}
