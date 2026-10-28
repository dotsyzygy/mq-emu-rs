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
auto eq_command(const char* name) -> decltype(CMDLIST::fAddress)
{
	if (!EQADDR_CMDLIST)
		return nullptr;
	for (int i = 0; EQADDR_CMDLIST[i].fAddress != nullptr; ++i)
	{
		if (EQADDR_CMDLIST[i].szName && mq::ci_equals(EQADDR_CMDLIST[i].szName, name))
			return EQADDR_CMDLIST[i].fAddress;
	}
	return nullptr;
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
