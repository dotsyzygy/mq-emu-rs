// Game data that only the game's own functions (or MacroQuest's) can answer:
// MacroQuest's cache of other spawns' buffs, alternate abilities, skill
// values and caps, the game's string table and extended target role names.
//
// This file imports MQ2Main.dll and eqlib.dll, so only plugins link it;
// mq-emu's tests use fakes instead. Every function checks for the game
// objects it needs and answers "nothing" when they're missing.
//
// No C++ exception may cross into Rust: every entry point catches them.

#include "mq_api.h"

#include "main/MQ2Main.h"
#include "mq/api/Abilities.h"
#include "eqlib/game/AltAbilities.h"
#include "eqlib/game/EQClasses.h"
#include "eqlib/game/Globals.h"
#include "eqlib/game/PcClient.h"

#include <algorithm>
#include <cstring>

using namespace eqlib;

// Mirrors `mqemu_cached_buff` in mq-emu-sys's game module.
struct mqemu_cached_buff
{
	int32_t slot;
	int32_t spell_id;
	// Ticks left when the server sent it; -1 for a buff that doesn't wear off.
	int32_t duration;
	// Hits left (counters).
	int32_t count;
	// What's left now, counting down from when it was sent.
	uint32_t remaining_ms;
	// How long ago the server sent it.
	uint32_t age_ms;
	char caster[64];
};

namespace {

bool copy_out(const char* text, char* out, size_t cap)
{
	if (cap == 0)
		return text != nullptr;
	if (text == nullptr)
	{
		out[0] = 0;
		return false;
	}
	size_t len = std::min(std::strlen(text), cap - 1);
	std::memcpy(out, text, len);
	out[len] = 0;
	return true;
}

} // namespace

// MacroQuest's cached buffs for `spawn` (${Spawn.Buff[n]} order), up to `cap`
// of them into `out`. Returns how many there are. The cache holds the buffs
// the server last sent for each spawn you targeted; expired ones are dropped.
extern "C" size_t mqemu_cached_buffs(PlayerClient* spawn, mqemu_cached_buff* out, size_t cap)
{
	try
	{
		if (spawn == nullptr)
			return 0;
		std::vector<mq::CachedBuff> buffs = mq::FilterCachedBuffs(spawn, [](const mq::CachedBuff&) { return true; });
		for (size_t i = 0; i < std::min(cap, buffs.size()); ++i)
		{
			const mq::CachedBuff& buff = buffs[i];
			out[i].slot = buff.slot;
			out[i].spell_id = buff.spellId;
			out[i].duration = buff.duration;
			out[i].count = buff.count;
			out[i].remaining_ms = buff.Duration();
			out[i].age_ms = buff.Staleness();
			copy_out(buff.casterName, out[i].caster, sizeof(out[i].caster));
		}
		return buffs.size();
	}
	catch (...)
	{
		return 0;
	}
}

// The alternate ability with this index (one rank of one ability), as
// AltAdvManager::GetAAById finds it. Null if there's none.
extern "C" CAltAbilityData* mqemu_alt_ability(int index)
{
	try
	{
		return pAltAdvManager ? pAltAdvManager->GetAAById(index) : nullptr;
	}
	catch (...)
	{
		return nullptr;
	}
}

// Whether you can use `ability` now, and its reuse time left in seconds
// (as ${Me.AltAbilityTimer} reads it). False with 0 if there's no character.
extern "C" bool mqemu_alt_ability_ready(CAltAbilityData* ability, int* reuse_seconds)
{
	*reuse_seconds = 0;
	try
	{
		if (!pAltAdvManager || !pLocalPC || ability == nullptr)
			return false;
		int refresh = 0;
		bool ready = pAltAdvManager->IsAbilityReady(pLocalPC, ability, &refresh);
		*reuse_seconds = std::max(refresh, 0);
		return ready;
	}
	catch (...)
	{
		return false;
	}
}

// The game's string `id` of `type` (eDatabaseStringType), NUL-terminated into
// `out` (cut to fit). False if the table has no such string.
extern "C" bool mqemu_db_string(int id, int type, char* out, size_t cap)
{
	try
	{
		if (!pDBStr)
			return copy_out(nullptr, out, cap);
		bool found = false;
		const char* text = pDBStr->GetString(id, static_cast<eDatabaseStringType>(type), &found);
		return copy_out(found ? text : nullptr, out, cap);
	}
	catch (...)
	{
		return copy_out(nullptr, out, cap);
	}
}

// Your skill `skill` (0 to NUM_SKILLS - 1). `which`: 0 the value with
// modifiers (${Me.Skill}), 1 the trained value (${Me.SkillBase}), 2 the cap at
// your level (${Me.SkillCap}). 0 if there's no character.
extern "C" int mqemu_skill_value(int skill, int which)
{
	try
	{
		if (!pLocalPC || skill < 0 || skill >= NUM_SKILLS)
			return 0;
		switch (which)
		{
		case 0: return pLocalPC->GetAdjustedSkill(skill);
		case 1: return pLocalPC->GetBaseSkill(skill);
		case 2: return pLocalPC->GetPcSkillLimit(skill);
		default: return 0;
		}
	}
	catch (...)
	{
		return 0;
	}
}

// Facts about skill `skill`, as bits: 1 you have it, 2 it's an ability you
// activate (a button on the abilities page), 4 it's ready to use now.
extern "C" uint32_t mqemu_skill_flags(int skill)
{
	try
	{
		if (!pLocalPC || !pSkillMgr || skill < 0 || skill >= NUM_SKILLS)
			return 0;
		uint32_t flags = 0;
		if (mq::HasSkill(skill))
			flags |= 1;
		if (mq::SkillIsActivatable(skill))
			flags |= 2;
		if (pSkillMgr->IsAvailable(skill))
			flags |= 4;
		return flags;
	}
	catch (...)
	{
		return 0;
	}
}

// Milliseconds until you can use skill `skill` again (${Me.AbilityTimer}),
// and in `total` its full reuse time (${Me.AbilityTimerTotal}).
extern "C" int mqemu_skill_timer(int skill, int* total)
{
	*total = 0;
	try
	{
		if (!pSkillMgr || skill < 0 || skill >= NUM_SKILLS)
			return 0;
		*total = static_cast<int>(pSkillMgr->GetSkillTimerDuration(skill));
		int left = *total - static_cast<int>(EQGetTime() - pSkillMgr->GetSkillLastUsed(skill));
		return std::max(left, 0);
	}
	catch (...)
	{
		return 0;
	}
}

// The skill's name as the game shows it, NUL-terminated into `out`. False
// for a skill with no name.
extern "C" bool mqemu_skill_name(int skill, char* out, size_t cap)
{
	try
	{
		return copy_out(skill >= 0 && skill < NUM_SKILLS ? mq::GetSkillName(skill) : nullptr, out, cap);
	}
	catch (...)
	{
		return copy_out(nullptr, out, cap);
	}
}

// The extended target window's name for a slot role (`xTargetType`: "Auto
// Hater", "Group Tank", ...), NUL-terminated into `out`.
extern "C" bool mqemu_xtarget_role_name(uint32_t role, char* out, size_t cap)
{
	try
	{
		if (!pLocalPC || !pLocalPC->pExtendedTargetList)
			return copy_out(nullptr, out, cap);
		return copy_out(pLocalPC->pExtendedTargetList->ExtendedTargetRoleName(role), out, cap);
	}
	catch (...)
	{
		return copy_out(nullptr, out, cap);
	}
}
