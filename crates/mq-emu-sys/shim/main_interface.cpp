// The only shim code that imports from MQ2Main.dll. It lives in its own file
// so that tests, which link the rest of the shim, don't need MQ2Main.dll to
// start: the linker only pulls this object in when something calls it.

#include "mq_api.h"

#include "main/MQ2Main.h"

extern "C" mq::MainInterface* mqemu_main_interface()
{
	return mq::GetMainInterface();
}

// The name MacroQuest gave a loaded plugin, looked up by any name it accepts
// for it ("MQ2Lua", "lua", "Lua.dll"...); null if it isn't loaded. Valid while
// that plugin stays loaded.
extern "C" const char* mqemu_loaded_plugin_name(const char* name)
{
	const mq::MQPlugin* plugin = name ? mq::GetPlugin(name) : nullptr;
	return plugin ? plugin->name.c_str() : nullptr;
}

// Targets `spawn`, or clears the target for null, as MacroQuest's /target
// does (/target clear for null): a plain write of pTarget, forgetting the old
// target's buffs.
extern "C" void mqemu_set_target(eqlib::PlayerClient* spawn)
{
	try
	{
		if (spawn != eqlib::pTarget)
			mq::gTargetbuffs = false;
		eqlib::pTarget = spawn;
		if (spawn == nullptr)
		{
			if (eqlib::pLocalPlayer)
				eqlib::pLocalPlayer->GroupMemberTargeted = -1;
			mq::SetSwitchTarget(nullptr);
			mq::ClearGroundSpawn();
		}
	}
	catch (...)
	{
	}
}
