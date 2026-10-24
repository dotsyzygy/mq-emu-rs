//! Signatures of the symbols a plugin DLL exports for MacroQuest to call.
//!
//! The loader (`MQPluginHandler.cpp`) looks each one up by name with
//! `GetProcAddress`; all are optional except the two sentinels.
//!
//! | Export name          | Signature                    |
//! |----------------------|------------------------------|
//! | `IsBuiltForNext`     | `bool` static (required)     |
//! | `EverQuestVersion`   | `char[]` static (required), must equal [`crate::EVERQUEST_VERSION`] |
//! | `InitPluginHandle`   | [`fMQInitPluginHandle`], called before `InitializePlugin` |
//! | `InitializePlugin`   | [`fMQInitializePlugin`]      |
//! | `ShutdownPlugin`     | [`fMQShutdownPlugin`]        |
//! | `OnPulse`            | [`fMQPulse`]                 |
//! | `OnIncomingChat`     | [`fMQIncomingChat`]          |
//! | `OnWriteChatColor`   | [`fMQWriteChatColor`]        |
//! | `OnZoned`            | [`fMQZoned`]                 |
//! | `OnCleanUI`          | [`fMQCleanUI`]               |
//! | `OnReloadUI`         | [`fMQReloadUI`]              |
//! | `OnDrawHUD`          | [`fMQDrawHUD`]               |
//! | `SetGameState`       | [`fMQSetGameState`]          |
//! | `OnAddSpawn`         | [`fMQSpawn`]                 |
//! | `OnRemoveSpawn`      | [`fMQSpawn`]                 |
//! | `OnAddGroundItem`    | [`fMQGroundItem`]            |
//! | `OnRemoveGroundItem` | [`fMQGroundItem`]            |
//! | `OnBeginZone`        | [`fMQBeginZone`]             |
//! | `OnEndZone`          | [`fMQEndZone`]               |
//! | `OnUpdateImGui`      | [`fMQUpdateImGui`]           |
//! | `OnMacroStart`       | [`fMQMacroStart`]            |
//! | `OnMacroStop`        | [`fMQMacroStop`]             |
//! | `OnLoadPlugin`       | [`fMQLoadPlugin`]            |
//! | `OnUnloadPlugin`     | [`fMQUnloadPlugin`]          |
//! | `OnPostUnloadPlugin` | [`fMQPostUnloadPlugin`]      |
//! | `GetPluginInterface` | [`fMQGetPluginInterface`]    |

use core::ffi::{c_char, c_int};

use crate::types::{EQGroundItem, MQPlugin, PlayerClient, PluginInterface};

/// `mq::MQPluginHandle` (`mq/base/PluginHandle.h`): identifies a plugin to
/// MacroQuest's `MainInterface`. MacroQuest passes it to `InitPluginHandle`.
///
/// The C++ type has user-defined copy/move constructors, but its layout is a
/// single `uint64_t`, which is all that crosses the boundary.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MQPluginHandle {
    pub pluginID: u64,
}

// Plugin callback signatures from `mq/api/PluginAPI.h`. `uint32_t` is `u32`.
/// Receives this plugin's record and handle before `InitializePlugin`.
pub type fMQInitPluginHandle = unsafe extern "C" fn(plugin: *mut MQPlugin, handle: MQPluginHandle);
pub type fMQWriteChatColor = unsafe extern "C" fn(line: *const c_char, color: u32, filter: u32) -> c_int;
pub type fMQPulse = unsafe extern "C" fn();
/// Return `true` to suppress the line from further processing.
pub type fMQIncomingChat = unsafe extern "C" fn(line: *const c_char, color: u32) -> bool;
pub type fMQInitializePlugin = unsafe extern "C" fn();
pub type fMQShutdownPlugin = unsafe extern "C" fn();
pub type fMQZoned = unsafe extern "C" fn();
pub type fMQReloadUI = unsafe extern "C" fn();
pub type fMQCleanUI = unsafe extern "C" fn();
pub type fMQDrawHUD = unsafe extern "C" fn();
/// One of the `GAMESTATE_*` constants.
pub type fMQSetGameState = unsafe extern "C" fn(game_state: c_int);
pub type fMQSpawn = unsafe extern "C" fn(spawn: *mut PlayerClient);
pub type fMQGroundItem = unsafe extern "C" fn(item: *mut EQGroundItem);
pub type fMQBeginZone = unsafe extern "C" fn();
pub type fMQEndZone = unsafe extern "C" fn();
pub type fMQUpdateImGui = unsafe extern "C" fn();
pub type fMQMacroStart = unsafe extern "C" fn(name: *const c_char);
pub type fMQMacroStop = unsafe extern "C" fn(name: *const c_char);
pub type fMQLoadPlugin = unsafe extern "C" fn(name: *const c_char);
pub type fMQUnloadPlugin = unsafe extern "C" fn(name: *const c_char);
/// Called after a plugin (possibly this one's peer) has been unloaded.
pub type fMQPostUnloadPlugin = unsafe extern "C" fn(name: *const c_char);
pub type fMQGetPluginInterface = unsafe extern "C" fn() -> *mut PluginInterface;

/// Export names of the required sentinel statics.
pub const EXPORT_IS_BUILT_FOR_NEXT: &str = "IsBuiltForNext";
pub const EXPORT_EVERQUEST_VERSION: &str = "EverQuestVersion";

/// Every optional function export the loader looks up, in the table above.
pub const CALLBACK_EXPORTS: &[&str] = &[
    "InitPluginHandle",
    "InitializePlugin",
    "ShutdownPlugin",
    "OnPulse",
    "OnIncomingChat",
    "OnWriteChatColor",
    "OnZoned",
    "OnCleanUI",
    "OnReloadUI",
    "OnDrawHUD",
    "SetGameState",
    "OnAddSpawn",
    "OnRemoveSpawn",
    "OnAddGroundItem",
    "OnRemoveGroundItem",
    "OnBeginZone",
    "OnEndZone",
    "OnUpdateImGui",
    "OnMacroStart",
    "OnMacroStop",
    "OnLoadPlugin",
    "OnUnloadPlugin",
    "OnPostUnloadPlugin",
    "GetPluginInterface",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_handle_is_a_bare_u64() {
        assert_eq!(size_of::<MQPluginHandle>(), 8);
        assert_eq!(align_of::<MQPluginHandle>(), align_of::<u64>());
    }
}
