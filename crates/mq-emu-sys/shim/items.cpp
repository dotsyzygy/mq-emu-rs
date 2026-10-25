// Game objects read through their C++ virtual functions, so the game's own
// implementation answers. An item the game created dispatches to the game's
// ItemClient::GetItemDefinition, which doesn't necessarily read the fields
// eqlib's inline version does.
//
// Only virtual calls: nothing here imports from MQ2Main.dll or eqlib.dll, so
// tests can link and exercise it with a fake vtable.

#include "mq_api.h"

#include "eqlib/game/Items.h"

extern "C" eqlib::ItemDefinition* mqemu_item_definition(const eqlib::ItemClient* item)
{
	return item ? item->GetItemDefinition() : nullptr;
}
