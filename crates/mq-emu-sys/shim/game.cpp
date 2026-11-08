// Game data that only the game's own functions (or MacroQuest's) can answer:
// MacroQuest's cache of other spawns' buffs, alternate abilities, skill
// values and caps, the game's string table and extended target role names;
// task progress and objective text, the friends list, tribute's cost and
// achievements; chat lines cleaned for event matching; and acting on spells
// (casting, memorizing, forgetting) and the game's own slash commands.
//
// This file imports MQ2Main.dll and eqlib.dll, so only plugins link it;
// mq-emu's tests use fakes instead. Every function checks for the game
// objects it needs and answers "nothing" when they're missing.
//
// No C++ exception may cross into Rust: every entry point catches them.

#include "mq_api.h"

#include "main/MQ2Main.h"
#include "mq/api/Abilities.h"
#include "eqlib/game/Achievements.h"
#include "eqlib/game/AltAbilities.h"
#include "eqlib/game/EQData.h"
#include "eqlib/game/EQClasses.h"
#include "eqlib/game/Globals.h"
#include "eqlib/game/ItemLinks.h"
#include "eqlib/game/PcClient.h"
#include "eqlib/game/TaskSystem.h"

#include <algorithm>
#include <cstring>
#include <string>

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

// Mirrors `mqemu_aura` in mq-emu-sys's game module.
struct mqemu_aura
{
	char name[64];
	uint32_t spawn_id;
	int32_t cost;
	int32_t icon_id;
};

// Your auras, as the aura window lists them (ClientAuraManager, what
// ${Me.Aura} reads): up to `cap` into `out`. Returns how many there are.
extern "C" size_t mqemu_auras(mqemu_aura* out, size_t cap)
{
	try
	{
		ClientAuraManager* auras = pAuraMgr;
		if (auras == nullptr)
			return 0;
		int count = auras->Auras.GetLength();
		for (int i = 0; i < count && static_cast<size_t>(i) < cap; ++i)
		{
			const AuraData& aura = auras->Auras[i];
			copy_out(aura.Name, out[i].name, sizeof(out[i].name));
			out[i].spawn_id = aura.SpawnID;
			out[i].cost = aura.Cost;
			out[i].icon_id = aura.IconID;
		}
		return static_cast<size_t>(std::max(count, 0));
	}
	catch (...)
	{
		return 0;
	}
}

// eqlib declares two skill functions with other arguments than this client's
// take, so a call through eqlib leaves the stack pointer 4 bytes off (and the
// caller's saved registers wrong; the debug window crashed on it). The game's
// own: GetBaseSkill ends in `ret 8` (it takes a second argument it doesn't
// read; eqlib passes one), GetPcSkillLimit in `ret 4` (one argument; eqlib
// passes two). These call them as the game does.
static int base_skill(int skill)
{
	using Fn = int(__thiscall*)(eqlib::CharacterZoneClient*, int, int);
	eqlib::CharacterZoneClient* character = static_cast<eqlib::PcClient*>(pLocalPC);
	return reinterpret_cast<Fn>(eqlib::CharacterZoneClient__GetBaseSkill)(character, skill, 0);
}

static int pc_skill_limit(int skill)
{
	using Fn = int(__thiscall*)(eqlib::PcZoneClient*, int);
	eqlib::PcZoneClient* pc = static_cast<eqlib::PcClient*>(pLocalPC);
	return reinterpret_cast<Fn>(eqlib::PcZoneClient__GetPcSkillLimit)(pc, skill);
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
		case 1: return base_skill(skill);
		case 2: return pc_skill_limit(skill);
		default: return 0;
		}
	}
	catch (...)
	{
		return 0;
	}
}

// How many bytes the stack pointer moves across one call of skill function
// `which` (1 GetBaseSkill, 2 GetPcSkillLimit) for skill `skill`, made through
// eqlib (`through_eqlib`) or as mqemu_skill_value makes it; 0 is right. For
// the selftest, which shows eqlib's calls are off on this client. The stack
// pointer is put back after, so the call itself does no harm.
extern "C" int mqemu_skill_call_stack_shift(int which, bool through_eqlib, int skill)
{
	if (!pLocalPC || skill < 0 || skill >= NUM_SKILLS || (which != 1 && which != 2))
		return 0;
	uintptr_t before = 0, after = 0;
	__asm mov before, esp
	if (through_eqlib)
		which == 1 ? pLocalPC->GetBaseSkill(skill) : pLocalPC->GetPcSkillLimit(skill);
	else
		which == 1 ? base_skill(skill) : pc_skill_limit(skill);
	__asm mov after, esp
	__asm mov esp, before
	return static_cast<int>(after - before);
}
