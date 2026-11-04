//! IDs in the RoF2 client's eqstr_us.txt, as [`StringTable::getString`]
//! (see [`GET_STRING`](crate::rof2::GET_STRING)) is asked for them.

/// "Your target is out of range, get closer!"
pub const TARGET_OUT_OF_RANGE: u32 = 100;
/// "This spell does not work here."
pub const SPELL_DOES_NOT_WORK_HERE: u32 = 106;
/// "You cannot see your target."
pub const CANNOT_SEE_TARGET: u32 = 108;
/// "Your spell fizzles!"
pub const SPELL_FIZZLES: u32 = 173;
/// "You can't cast spells while invulnerable!"
pub const CANT_CAST_INVULNERABLE: u32 = 174;
/// "You can't cast spells while stunned!"
pub const CANT_CAST_STUNNED: u32 = 175;
/// "Your target has no mana to affect"
pub const TARGET_HAS_NO_MANA: u32 = 191;
/// "You must first target a group member."
pub const MUST_TARGET_GROUP_MEMBER: u32 = 196;
/// "Your spell is too powerful for your intended target."
pub const SPELL_TOO_POWERFUL: u32 = 197;
/// "Insufficient Mana to cast this spell!"
pub const INSUFFICIENT_MANA: u32 = 199;
/// "You *CANNOT* cast spells, you have been silenced!"
pub const SILENCED: u32 = 207;
/// "You must first select a target for this spell!"
pub const MUST_SELECT_TARGET: u32 = 214;
/// "This spell only works on summoned beings." through "...the dragons."
pub const ONLY_WORKS_ON: std::ops::RangeInclusive<u32> = 216..=229;
/// "You can only cast this spell in the outdoors."
pub const ONLY_OUTDOORS: u32 = 234;
/// "Spell recast time not yet met."
pub const RECAST_NOT_MET: u32 = 236;
/// "Spell recovery time not yet met."
pub const RECOVERY_NOT_MET: u32 = 237;
/// "Your target cannot be mesmerized."
pub const CANNOT_BE_MESMERIZED: u32 = 239;
/// "Your target cannot be mesmerized (with this spell)."
pub const CANNOT_BE_MESMERIZED_BY_THIS: u32 = 240;
/// "Your target is immune to the stun portion of this effect."
pub const IMMUNE_TO_STUN: u32 = 241;
/// "Your target is immune to changes in its attack speed."
pub const IMMUNE_TO_SLOW: u32 = 242;
/// "Your target is immune to fear spells."
pub const IMMUNE_TO_FEAR: u32 = 243;
/// "Your target is immune to changes in its run speed."
pub const IMMUNE_TO_SNARE: u32 = 244;
/// "Your spell did not take hold."
pub const SPELL_DID_NOT_TAKE_HOLD: u32 = 263;
/// "Your target looks unaffected."
pub const TARGET_UNAFFECTED: u32 = 268;
/// "Your spell would not have taken hold on your target."
pub const SPELL_WOULD_NOT_TAKE_HOLD: u32 = 271;
/// "You are missing some required components."
pub const MISSING_COMPONENTS: u32 = 272;
/// "You need to play a percussion instrument for this song." through brass.
pub const NEED_INSTRUMENT: std::ops::RangeInclusive<u32> = 405..=408;
/// "Your target resisted the %1 spell."
pub const TARGET_RESISTED: u32 = 425;
/// "Your spell is interrupted."
pub const SPELL_INTERRUPTED: u32 = 439;
/// "Your target is immune to the %1 spell."
pub const IMMUNE_TO_SPELL: u32 = 612;
/// "You haven't recovered yet..."
pub const NOT_RECOVERED: u32 = 12029;
/// "You begin casting %1."
pub const BEGIN_CASTING: u32 = 12205;
/// "You are too distracted to cast a spell now!"
pub const TOO_DISTRACTED_TO_CAST: u32 = 12440;
/// "You must be standing to cast a spell."
pub const MUST_BE_STANDING: u32 = 12441;
/// "Your casting has been interrupted!"
pub const CASTING_INTERRUPTED: u32 = 12477;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every constant above, with the text eqstr_us.txt must have for it.
    const TEXTS: &[(u32, &str)] = &[
        (TARGET_OUT_OF_RANGE, "Your target is out of range, get closer!"),
        (SPELL_DOES_NOT_WORK_HERE, "This spell does not work here."),
        (CANNOT_SEE_TARGET, "You cannot see your target."),
        (SPELL_FIZZLES, "Your spell fizzles!"),
        (CANT_CAST_INVULNERABLE, "You can't cast spells while invulnerable!"),
        (CANT_CAST_STUNNED, "You can't cast spells while stunned!"),
        (TARGET_HAS_NO_MANA, "Your target has no mana to affect"),
        (MUST_TARGET_GROUP_MEMBER, "You must first target a group member."),
        (SPELL_TOO_POWERFUL, "Your spell is too powerful for your intended target."),
        (INSUFFICIENT_MANA, "Insufficient Mana to cast this spell!"),
        (SILENCED, "You *CANNOT* cast spells, you have been silenced!"),
        (MUST_SELECT_TARGET, "You must first select a target for this spell!"),
        (216, "This spell only works on summoned beings."),
        (229, "This spell only works on the dragons."),
        (ONLY_OUTDOORS, "You can only cast this spell in the outdoors."),
        (RECAST_NOT_MET, "Spell recast time not yet met."),
        (RECOVERY_NOT_MET, "Spell recovery time not yet met."),
        (CANNOT_BE_MESMERIZED, "Your target cannot be mesmerized."),
        (CANNOT_BE_MESMERIZED_BY_THIS, "Your target cannot be mesmerized (with this spell)."),
        (IMMUNE_TO_STUN, "Your target is immune to the stun portion of this effect."),
        (IMMUNE_TO_SLOW, "Your target is immune to changes in its attack speed."),
        (IMMUNE_TO_FEAR, "Your target is immune to fear spells."),
        (IMMUNE_TO_SNARE, "Your target is immune to changes in its run speed."),
        (SPELL_DID_NOT_TAKE_HOLD, "Your spell did not take hold."),
        (TARGET_UNAFFECTED, "Your target looks unaffected."),
        (SPELL_WOULD_NOT_TAKE_HOLD, "Your spell would not have taken hold on your target."),
        (MISSING_COMPONENTS, "You are missing some required components."),
        (405, "You need to play a percussion instrument for this song."),
        (408, "You need to play a brass instrument for this song."),
        (TARGET_RESISTED, "Your target resisted the %1 spell."),
        (SPELL_INTERRUPTED, "Your spell is interrupted."),
        (IMMUNE_TO_SPELL, "Your target is immune to the %1 spell."),
        (NOT_RECOVERED, "You haven't recovered yet..."),
        (BEGIN_CASTING, "You begin casting %1."),
        (TOO_DISTRACTED_TO_CAST, "You are too distracted to cast a spell now!"),
        (MUST_BE_STANDING, "You must be standing to cast a spell."),
        (CASTING_INTERRUPTED, "Your casting has been interrupted!"),
    ];

    #[test]
    fn the_ids_have_the_clients_texts() {
        let path = std::env::var("EQ_STR_US").unwrap_or_else(|_| r"D:\Games\ProFusionEQ\eqstr_us.txt".into());
        let Ok(file) = std::fs::read(&path) else {
            eprintln!("skipped: no eqstr_us.txt (set EQ_STR_US)");
            return;
        };
        let file = String::from_utf8_lossy(&file);
        for (id, text) in TEXTS {
            let prefix = format!("{id} ");
            let line = file.lines().find(|line| line.starts_with(&prefix)).unwrap_or_else(|| panic!("{id} isn't in {path}"));
            assert_eq!(line[prefix.len()..].trim_end(), *text, "{id}");
        }
    }
}
