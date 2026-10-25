// The only shim code that imports from MQ2Main.dll. It lives in its own file
// so that tests, which link the rest of the shim, don't need MQ2Main.dll to
// start: the linker only pulls this object in when something calls it.

#include "mq_api.h"

extern "C" mq::MainInterface* mqemu_main_interface()
{
	return mq::GetMainInterface();
}
