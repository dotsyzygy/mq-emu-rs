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

// The game's own handler for slash command `name`, from its command table, as
// MacroQuest finds /cast and /useitem's (cmdCast, cmdUseItem): not
// MacroQuest's command of that name, nor an alias. Null if it has none.
// The game's handler for slash command `name`: the command of that name, or
// else the first in the game's list that `name` abbreviates, as typing it
// does ("/alt" for the game's longer name).
auto eq_command(const char* name) -> decltype(CMDLIST::fAddress)
{
	if (!EQADDR_CMDLIST || name == nullptr || name[0] == 0)
		return nullptr;
	decltype(CMDLIST::fAddress) abbreviated = nullptr;
	for (int i = 0; EQADDR_CMDLIST[i].fAddress != nullptr; ++i)
	{
		const char* command = EQADDR_CMDLIST[i].szName;
		if (command == nullptr)
			continue;
		if (mq::ci_equals(command, name))
			return EQADDR_CMDLIST[i].fAddress;
		if (abbreviated == nullptr && mq::ci_starts_with(command, name))
			abbreviated = EQADDR_CMDLIST[i].fAddress;
	}
	return abbreviated;
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

// Your maximum endurance with its modifiers (${Me.MaxEndurance}). 0 if there's
// no character.
extern "C" int mqemu_max_endurance()
{
	try
	{
		return pLocalPC ? pLocalPC->Max_Endurance() : 0;
	}
	catch (...)
	{
		return 0;
	}
}

// Your current HP (${Me.CurrentHPs}, GetCurHPS). 0 if there's no character.
extern "C" int mqemu_hp_current()
{
	try
	{
		return pLocalPC ? pLocalPC->Cur_HP(0) : 0;
	}
	catch (...)
	{
		return 0;
	}
}

// Your maximum HP with its modifiers (${Me.MaxHPs}, GetMaxHPS). 0 if there's
// no character.
extern "C" int mqemu_max_hp()
{
	try
	{
		return pLocalPC ? pLocalPC->Max_HP(0) : 0;
	}
	catch (...)
	{
		return 0;
	}
}

// Your maximum mana with its modifiers (${Me.MaxMana}, GetMaxMana). 0 if
// there's no character.
extern "C" int mqemu_max_mana()
{
	try
	{
		return pLocalPC ? pLocalPC->Max_Mana() : 0;
	}
	catch (...)
	{
		return 0;
	}
}

// The name ${Spawn.Class} shows for a class ID. False for none.
extern "C" bool mqemu_class_name(int32_t class_id, char* out, size_t cap)
{
	try
	{
		return copy_out(pEverQuest ? mq::GetClassDesc(class_id) : nullptr, out, cap);
	}
	catch (...)
	{
		return copy_out(nullptr, out, cap);
	}
}

// The name ${Spawn.Race} shows for a race ID. False for none.
extern "C" bool mqemu_race_name(int32_t race, char* out, size_t cap)
{
	try
	{
		return copy_out(pEverQuest ? pEverQuest->GetRaceDesc(static_cast<EQRace>(race)) : nullptr, out, cap);
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

// A chat line as MQ2Lua matches events against it: item links replaced by
// their text (CleanItemTags, the game's own) unless `keep_links`, then
// MacroQuest's color codes and line breaks removed (StripMQChat). Into
// `out`, which needs strlen(line) + 1 bytes: neither step lengthens it.
extern "C" bool mqemu_clean_chat_line(const char* line, bool keep_links, char* out, size_t cap)
{
	try
	{
		std::string text = line;
		if (!keep_links && text.find('\x12') != std::string::npos)
			text = CleanItemTags(CXStr(text), false).c_str();
		std::string stripped(text.size() + 1, '\0');
		mq::StripMQChat(text.c_str(), stripped.data());
		return copy_out(stripped.c_str(), out, cap);
	}
	catch (...)
	{
		return copy_out(nullptr, out, cap);
	}
}

// ----- acting on spells, and the game's own commands -----

// Runs the game's own handler for slash command `name` ("/autoinventory")
// with `args` (what follows the command), as you: what typing it does once
// MacroQuest passes it on. False if the game has no such command or there's
// no character.
extern "C" bool mqemu_eq_command(const char* name, const char* args)
{
	try
	{
		auto command = eq_command(name);
		if (command == nullptr || !pLocalPlayer)
			return false;
		command(pLocalPlayer, args);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// Memorizes spells into gems as /memspell does (CSpellBookWnd::MemorizeSet):
// `spells` has a spell ID for each of the NUM_SPELL_GEMS gems, -1 to leave a
// gem as it is. The game memorizes them one after another, sitting you down.
// False without the spell book window or with another count.
extern "C" bool mqemu_memorize_spells(const int* spells, size_t count)
{
	try
	{
		if (!pSpellBookWnd || !pLocalPC || count != NUM_SPELL_GEMS)
			return false;
		int loadout[NUM_SPELL_GEMS];
		std::copy(spells, spells + NUM_SPELL_GEMS, loadout);
		pSpellBookWnd->MemorizeSet(loadout, NUM_SPELL_GEMS);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// Forgets the spell in gem `gem` (0-based), as right-clicking the gem does
// (CCastSpellWnd::ForgetMemorizedSpell). False if the gem is empty. The
// client stops on a breakpoint (int3) for gems from 16 on, so only the
// NUM_SPELL_GEMS gems are passed on.
extern "C" bool mqemu_forget_gem(int gem)
{
	try
	{
		if (!pCastSpellWnd || !pLocalPC || gem < 0 || gem >= NUM_SPELL_GEMS || mq::GetMemorizedSpell(gem) == -1)
			return false;
		pCastSpellWnd->ForgetMemorizedSpell(gem);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// Whether the spell in gem `gem` (0-based) can be cast now, as
// ${Me.SpellReady[n]} says: not while the casting window shows (you're
// casting), always while a bard song plays, otherwise once the gem's refresh
// and the global cooldown are over. False for an empty gem.
extern "C" bool mqemu_gem_ready(int gem)
{
	try
	{
		if (!pDisplay || !pLocalPlayer || gem < 0 || gem >= NUM_SPELL_GEMS || !mq::GetSpellByID(mq::GetMemorizedSpell(gem)))
			return false;
		if (pCastingWnd && pCastingWnd->IsVisible())
			return false;
		if (pCastSpellWnd && pCastSpellWnd->IsBardSongPlaying())
			return true;
		return pDisplay->TimeStamp > pLocalPlayer->SpellGemETA[gem] && pDisplay->TimeStamp > pLocalPlayer->GetSpellCooldownETA();
	}
	catch (...)
	{
		return false;
	}
}

// Uses skill `skill` on your target, as /doability <name> does
// (PcClient::UseSkill), if you have it and it's an ability you activate. The
// game checks its timer and the rest itself. False otherwise, or without a
// character.
extern "C" bool mqemu_use_skill(int skill)
{
	try
	{
		if (!pLocalPC || !pLocalPlayer || skill < 0 || skill >= NUM_SKILLS)
			return false;
		if (!mq::HasSkillOrInnate(skill) || !mq::SkillOrInnateIsActivatable(skill))
			return false;
		pLocalPC->UseSkill(static_cast<uint8_t>(skill), pLocalPlayer);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// Uses the combat ability (discipline) `spell_id` as /doability does
// (PcZoneClient::DoCombatAbility, allowing a lower rank). The game checks
// timers, endurance and the rest itself, and says why in chat when it won't.
// False without a character or when the game refuses.
extern "C" bool mqemu_do_combat_ability(int spell_id)
{
	try
	{
		if (!pLocalPC || spell_id <= 0)
			return false;
		return pLocalPC->DoCombatAbility(spell_id);
	}
	catch (...)
	{
		return false;
	}
}

// Milliseconds left on your buff (or with `song`, song) from spell
// `spell_id`, as its window counts it down each frame. GetSpellBuffTimer
// does this, but looks in the buff window first even for a song. -1 if it
// isn't in that window.
extern "C" int mqemu_buff_timer(int spell_id, bool song)
{
	try
	{
		CBuffWindow* window = song ? pSongWnd : pBuffWnd;
		if (!window || spell_id <= 0)
			return -1;
		auto info = window->GetBuffInfoBySpellID(spell_id);
		return info ? info.GetBuffTimer() : -1;
	}
	catch (...)
	{
		return -1;
	}
}

// How close `attacker` must be to hit `target` in melee, as
// ${Spawn.MaxRangeTo} works it out (mq::GetMeleeRange, from both spawns'
// sizes and heights; 14 to 75). 14 for a null spawn.
extern "C" float mqemu_melee_range(eqlib::PlayerClient* attacker, eqlib::PlayerClient* target)
{
	try
	{
		return mq::GetMeleeRange(attacker, target);
	}
	catch (...)
	{
		return 14.0f;
	}
}

// Your progress on `task` (one of pTaskManager's QuestEntries or
// SharedTaskEntries), as ${Task[n].Objective[m].Status} reads it
// (CTaskManager::GetTaskStatus). Null for an entry in neither, or without a
// character.
extern "C" PCTaskStatus* mqemu_task_status(CTaskEntry* task)
{
	try
	{
		if (!pTaskManager || !pLocalPC || task == nullptr)
			return nullptr;
		for (int i = 0; i < MAX_QUEST_ENTRIES; ++i)
		{
			if (&pTaskManager->QuestEntries[i] == task)
				return pTaskManager->GetTaskStatus(pLocalPC, i, cTaskSystemTypeSoloQuest);
		}
		for (int i = 0; i < MAX_SHARED_TASK_ENTRIES; ++i)
		{
			if (&pTaskManager->SharedTaskEntries[i] == task)
				return pTaskManager->GetTaskStatus(pLocalPC, i, cTaskSystemTypeSharedQuest);
		}
		return nullptr;
	}
	catch (...)
	{
		return nullptr;
	}
}

// What a task objective asks, as the task window says it
// (CTaskManager::GetElementDescription, ${Task[n].Objective[m]}). False
// without the task manager or for an objective with no text.
extern "C" bool mqemu_task_objective_text(const CTaskElement* objective, char* out, size_t cap)
{
	try
	{
		if (!pTaskManager || objective == nullptr)
			return copy_out(nullptr, out, cap);
		// The game writes up to MAX_STRING, whatever the caller's size.
		char text[MAX_STRING] = { 0 };
		pTaskManager->GetElementDescription(objective, text);
		return copy_out(text[0] ? text : nullptr, out, cap);
	}
	catch (...)
	{
		return copy_out(nullptr, out, cap);
	}
}

// The game's clock in seconds (GetFastTime), which task timers count by.
extern "C" uint32_t mqemu_fast_time()
{
	try
	{
		return GetFastTime();
	}
	catch (...)
	{
		return 0;
	}
}

// How many friends your friends list has (${Friends}); -1 without the chat
// service that keeps it.
extern "C" int mqemu_friend_count()
{
	try
	{
		return pChatService ? pChatService->GetNumberOfFriends() : -1;
	}
	catch (...)
	{
		return -1;
	}
}

// Your friend `index` (0-based), as ${Friends.Friend[index+1]}. False past
// the end.
extern "C" bool mqemu_friend_name(int index, char* out, size_t cap)
{
	try
	{
		UniversalChatProxy* chat = pChatService;
		if (!chat || index < 0 || index >= chat->GetNumberOfFriends())
			return copy_out(nullptr, out, cap);
		return copy_out(chat->GetFriendName(index), out, cap);
	}
	catch (...)
	{
		return copy_out(nullptr, out, cap);
	}
}

// What your active tribute costs in favor (CTribute::GetActiveFavorCost,
// ${Me.ActiveFavorCost}). 0 without the tribute manager.
extern "C" int mqemu_active_favor_cost()
{
	try
	{
		return pTribute ? pTribute->GetActiveFavorCost() : 0;
	}
	catch (...)
	{
		return 0;
	}
}

// The achievement manager (AchievementManager::Instance): every achievement
// and category the client knows, and your progress.
extern "C" AchievementManager* mqemu_achievement_manager()
{
	try
	{
		return &AchievementManager::Instance();
	}
	catch (...)
	{
		return nullptr;
	}
}

// A chat link to `achievement` as `player` has it (FormatAchievementLink,
// ${Achievement[...].Link}).
extern "C" bool mqemu_achievement_link(const Achievement* achievement, const char* player, char* out, size_t cap)
{
	try
	{
		if (achievement == nullptr || cap == 0)
			return copy_out(nullptr, out, cap);
		FormatAchievementLink(out, cap, achievement, player);
		return out[0] != 0;
	}
	catch (...)
	{
		return copy_out(nullptr, out, cap);
	}
}
