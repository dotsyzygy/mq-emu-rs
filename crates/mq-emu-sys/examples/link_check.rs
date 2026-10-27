//! Link-time check: a DLL that references every import, so the linker has to
//! resolve each one against MQ2Main.lib / eqlib.lib. `cargo test` builds
//! examples, so a bad declaration fails the test run even though nothing here
//! is ever executed.

use core::ffi::c_void;

use mq_emu_sys::*;

static IMGUI_MQ: mq_emu_sys::imgui_mq::Api = mq_emu_sys::imgui_mq::API;
static GAME: mq_emu_sys::game::Api = mq_emu_sys::game::API;
static WINDOWS: mq_emu_sys::windows::Api = mq_emu_sys::windows::API;

#[unsafe(no_mangle)]
pub extern "C" fn mq_emu_sys_link_check() -> usize {
    let functions: &[*const c_void] = &[
        WriteChatf as *const c_void,
        WriteChatColor as *const c_void,
        WriteChatColorf as *const c_void,
        DebugSpew as *const c_void,
        DebugSpewAlways as *const c_void,
        ParseMacroData as *const c_void,
        GetGameState as *const c_void,
        GetPluginProc as *const c_void,
        FindMappableCommand as *const c_void,
        AddMQ2KeyBind as *const c_void,
        RemoveMQ2KeyBind as *const c_void,
        SetMQ2KeyBind as *const c_void,
        GetMQ2KeyBind as *const c_void,
        PressMQ2KeyBind as *const c_void,
        SetEQKeyBind as *const c_void,
        ParseKeyCombo as *const c_void,
        DescribeKeyCombo as *const c_void,
        DumpBinds as *const c_void,
        StripMQChat as *const c_void,
        AddXMLFile as *const c_void,
        RemoveXMLFile as *const c_void,
        IsScreenPieceLoaded as *const c_void,
        EQExecuteCmd as *const c_void,
    ];
    let statics: &[*const c_void] = &[
        (&raw const gGameState).cast(),
        (&raw const gFaceAngle).cast(),
        (&raw const gLookAngle).cast(),
        (&raw const gbInZone).cast(),
        (&raw const gszMacroName).cast(),
        (&raw const gPathMQRoot).cast(),
        (&raw const gPathConfig).cast(),
        (&raw const gPathMacros).cast(),
        (&raw const gPathLogs).cast(),
        (&raw const gPathPlugins).cast(),
        (&raw const gPathResources).cast(),
        (&raw const gTargetbuffs).cast(),
        (&raw const pLocalPlayer).cast(),
        (&raw const pControlledPlayer).cast(),
        (&raw const pTarget).cast(),
        (&raw const pSpawnManager).cast(),
        (&raw const pKeypressHandler).cast(),
        (&raw const szEQMappableCommands).cast(),
        (&raw const pEverQuestInfo).cast(),
        (&raw const EQbCommandStates).cast(),
        (&raw const pLocalPC).cast(),
        (&raw const pSpellMgr).cast(),
        (&raw const pWorldData).cast(),
        (&raw const pZoneInfo).cast(),
        (&raw const pItemList).cast(),
        (&raw const pTargetWnd).cast(),
        (&raw const pPetInfoWnd).cast(),
        (&raw const pRaid).cast(),
        (&raw const pMercManager).cast(),
        (&raw const pAggroInfo).cast(),
        (&raw const pSkillMgr).cast(),
        (&raw const pMerchantWnd).cast(),
        (&raw const pLootWnd).cast(),
        (&raw const pActiveMerchant).cast(),
        (&raw const pActiveCorpse).cast(),
        (&raw const pWndMgr).cast(),
        (&raw const pContextMenuManager).cast(),
        (&raw const pInvSlotMgr).cast(),
        (&raw const pTradeWnd).cast(),
    ];
    assert_eq!(
        functions.len() + statics.len(),
        MQ2MAIN_IMPORTS.len() + EQLIB_IMPORTS.len(),
        "link_check is missing an import"
    );
    // The shim's MQ2Main and eqlib imports are C++-mangled: mq::GetMainInterface,
    // and for ImGui the texture, texture animation and font functions (every
    // one of them reached through the table), and the game data functions
    // (cached buffs, alternate abilities, skills, the string table) and EQ UI
    // window functions.
    let shim: &[*const c_void] = &[
        mq_emu_sys::shim::mqemu_main_interface as *const c_void,
        mq_emu_sys::shim::mqemu_add_command as *const c_void,
        mq_emu_sys::shim::mqemu_do_command as *const c_void,
        (&raw const IMGUI_MQ).cast(),
        (&raw const GAME).cast(),
        (&raw const WINDOWS).cast(),
    ];
    functions.iter().chain(statics).chain(shim).map(|p| *p as usize).fold(0, usize::wrapping_add)
}
