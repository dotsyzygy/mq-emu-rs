//! Where eq-client hooks the RoF2 client (eqgame.exe, `__ClientDate`
//! 20130510), at its preferred base. Each hook names the exact bytes it
//! replaces, so a client build where they differ is refused, not patched.

/// Where eqgame.exe asks to be loaded; addresses below are relative to it.
pub const PREFERRED_BASE: usize = 0x40_0000;

/// A function eq-client detours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookSite {
    pub name: &'static str,
    /// At [`PREFERRED_BASE`].
    pub address: usize,
    /// The whole, position-independent instructions the detour replaces.
    pub prologue: &'static [u8],
}

impl HookSite {
    /// The address in a client loaded at `base`.
    pub fn at(&self, base: usize) -> usize {
        self.address - PREFERRED_BASE + base
    }
}

/// `const char* StringTable::getString(uint32_t id, bool* found)`, thiscall:
/// eqstr_us.txt's text for an ID. Every message the client shows by ID
/// passes through it: the server's (`msgTokenText`, `msgTokenTextParam`) and
/// the client's own (spell casting's checks, say). MacroQuest doesn't hook it.
pub const GET_STRING: HookSite = HookSite {
    name: "StringTable::getString",
    address: 0x7D_0660,
    // push ebp; mov ebp,esp; mov eax,[ecx+8]
    prologue: &[0x55, 0x8B, 0xEC, 0x8B, 0x41, 0x08],
};

/// Every hook site, for checking them against a client.
pub const SITES: &[HookSite] = &[GET_STRING];

#[cfg(test)]
mod tests {
    use super::*;

    /// The client to check against: `EQ_GAME_EXE`, or ProFusionEQ's.
    fn client() -> Option<Vec<u8>> {
        let path = std::env::var("EQ_GAME_EXE").unwrap_or_else(|_| r"D:\Games\ProFusionEQ\eqgame.exe".into());
        std::fs::read(path).ok()
    }

    /// The file offset of a virtual address, from the PE section table.
    fn file_offset(exe: &[u8], address: usize) -> Option<usize> {
        let u16_at = |at: usize| u16::from_le_bytes([exe[at], exe[at + 1]]) as usize;
        let u32_at = |at: usize| u32::from_le_bytes(exe[at..at + 4].try_into().unwrap()) as usize;
        let pe = u32_at(0x3C);
        let sections = u16_at(pe + 6);
        let first = pe + 24 + u16_at(pe + 20);
        let rva = address - PREFERRED_BASE;
        (0..sections).map(|n| first + n * 40).find_map(|header| {
            let (size, start, raw) = (u32_at(header + 8), u32_at(header + 12), u32_at(header + 20));
            (start..start + size).contains(&rva).then(|| rva - start + raw)
        })
    }

    #[test]
    fn the_hooked_bytes_are_the_clients() {
        let Some(exe) = client() else {
            eprintln!("skipped: no eqgame.exe (set EQ_GAME_EXE)");
            return;
        };
        for site in SITES {
            let at = file_offset(&exe, site.address).expect("in a section");
            assert_eq!(&exe[at..at + site.prologue.len()], site.prologue, "{}", site.name);
        }
    }

    #[test]
    fn addresses_follow_the_base() {
        assert_eq!(GET_STRING.at(PREFERRED_BASE), 0x7D_0660);
        assert_eq!(GET_STRING.at(0x100_0000), 0x13D_0660);
    }
}
