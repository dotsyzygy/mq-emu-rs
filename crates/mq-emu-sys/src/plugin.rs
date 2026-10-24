//! Signatures of the callbacks a plugin DLL exports for MacroQuest to call.
//!
//! The loader (`MQ2PluginHandler.cpp`) looks each one up by name with
//! `GetProcAddress`; all are optional except the two sentinels below.
//!
//! | Export name          | Signature                    |
//! |----------------------|------------------------------|
//! | `IsBuiltForNext`     | `bool` static (required)     |
//! | `EverQuestVersion`   | `char[]` static (required), must equal [`crate::EVERQUEST_VERSION`] |
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
//! | `GetPluginInterface` | [`fMQGetPluginInterface`]    |

use core::ffi::c_char;

use crate::types::{EQGroundItem, PlayerClient, PluginInterface};

/// Plugin callback signatures from `MQ2Internal.h`. Win32 `DWORD` is `u32`.
pub type fMQWriteChatColor = unsafe extern "C" fn(line: *const c_char, color: u32, filter: u32) -> u32;
pub type fMQPulse = unsafe extern "C" fn();
/// Return `true` to suppress the line from further processing.
pub type fMQIncomingChat = unsafe extern "C" fn(line: *const c_char, color: u32) -> bool;
pub type fMQInitializePlugin = unsafe extern "C" fn();
pub type fMQShutdownPlugin = unsafe extern "C" fn();
pub type fMQZoned = unsafe extern "C" fn();
pub type fMQReloadUI = unsafe extern "C" fn();
pub type fMQCleanUI = unsafe extern "C" fn();
pub type fMQDrawHUD = unsafe extern "C" fn();
pub type fMQSetGameState = unsafe extern "C" fn(game_state: u32);
pub type fMQSpawn = unsafe extern "C" fn(spawn: *mut PlayerClient);
pub type fMQGroundItem = unsafe extern "C" fn(item: *mut EQGroundItem);
pub type fMQBeginZone = unsafe extern "C" fn();
pub type fMQEndZone = unsafe extern "C" fn();
pub type fMQUpdateImGui = unsafe extern "C" fn();
pub type fMQMacroStart = unsafe extern "C" fn(name: *const c_char);
pub type fMQMacroStop = unsafe extern "C" fn(name: *const c_char);
pub type fMQLoadPlugin = unsafe extern "C" fn(name: *const c_char);
pub type fMQUnloadPlugin = unsafe extern "C" fn(name: *const c_char);
pub type fMQGetPluginInterface = unsafe extern "C" fn() -> *mut PluginInterface;

/// Export names of the required sentinel statics.
pub const EXPORT_IS_BUILT_FOR_NEXT: &str = "IsBuiltForNext";
pub const EXPORT_EVERQUEST_VERSION: &str = "EverQuestVersion";
