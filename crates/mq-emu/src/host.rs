//! The few runtime operations that call into MacroQuest, behind function
//! pointers so tests can substitute fakes. Code reachable from tests must go
//! through [`Host`] rather than calling MQ2Main.dll directly; otherwise the
//! test binary would need MQ2Main.dll to start.
//!
//! The real functions are generic over the plugin type (`P`, otherwise
//! unused), so their code is only generated where a plugin instantiates
//! `Runtime::new`. A plain function would be compiled into mq-emu's library,
//! and any binary linking the object file it landed in, a test using only
//! `mq_emu::lua` say, would import MQ2Main.dll and eqlib.dll.

// The unused `P` on the real functions is the point (see above).
#![allow(clippy::extra_unused_type_parameters)]

use std::time::Instant;

use mq_emu_sys::constants::MAX_STRING;
use mq_emu_sys::constants::N_EQ_MAPPABLE_COMMANDS;
use mq_emu_sys::layouts::{
    AggroMeterManagerClient, CLootWnd, CMercenaryManager, CMerchantWnd, CPetInfoWnd, CRaid, CTargetWnd, ClientSpellManager,
    EQGroundItemListManager, EQWorldData, EverQuestinfo, PcClient, PlayerClient, PlayerManagerClient, zoneHeader,
};
use mq_emu_sys::shim::MainInterface;
use mq_emu_sys::types::{ComputedPointer, ForeignPointer};

use crate::text::to_c;

#[derive(Clone, Copy)]
pub(crate) struct Host {
    pub write_chat: fn(&str, i32),
    pub main_interface: fn() -> *mut MainInterface,
    pub game_state: fn() -> i32,
    pub now: fn() -> Instant,
    pub local_player: fn() -> *mut PlayerClient,
    /// The spawn whose movement the player controls (a mount when riding).
    pub controlled_player: fn() -> *mut PlayerClient,
    pub target: fn() -> *mut PlayerClient,
    /// Owns the zone's spawn list.
    pub spawn_manager: fn() -> *mut PlayerManagerClient,
    /// Your character's data (`pLocalPC`): profile, bank, group.
    pub local_pc: fn() -> *mut PcClient,
    /// The spell table (`pSpellMgr`).
    pub spell_manager: fn() -> *mut ClientSpellManager,
    /// Every zone the client knows (`pWorldData`).
    pub world_data: fn() -> *mut EQWorldData,
    /// The zone you're in (`pZoneInfo`).
    pub zone_info: fn() -> *mut zoneHeader,
    /// The ground item list (`pItemList`).
    pub ground_items: fn() -> *mut EQGroundItemListManager,
    /// Client-wide state; `keyDown` says which game commands are down.
    pub everquest_info: fn() -> *mut EverQuestinfo,
    /// Whether the server has sent the current target's buffs (`gTargetbuffs`).
    pub target_buffs_populated: fn() -> bool,
    /// The target window (`pTargetWnd`).
    pub target_window: fn() -> *mut CTargetWnd,
    /// The pet window (`pPetInfoWnd`).
    pub pet_window: fn() -> *mut CPetInfoWnd,
    /// Your raid (`pRaid`).
    pub raid: fn() -> *mut CRaid,
    /// Your mercenary (`pMercManager`).
    pub mercenary_manager: fn() -> *mut CMercenaryManager,
    /// Aggro percentages (`pAggroInfo`).
    pub aggro: fn() -> *mut AggroMeterManagerClient,
    /// The merchant window (`pMerchantWnd`).
    pub merchant_window: fn() -> *mut CMerchantWnd,
    /// The loot window (`pLootWnd`).
    pub loot_window: fn() -> *mut CLootWnd,
    /// The merchant whose window is open (`pActiveMerchant`).
    pub active_merchant: fn() -> *mut PlayerClient,
    /// The corpse being looted (`pActiveCorpse`).
    pub active_corpse: fn() -> *mut PlayerClient,
    /// The game's own functions for what layouts can't answer (`mq_emu_sys::game`).
    pub game_fns: &'static mq_emu_sys::game::Api,
    /// Whether the game currently allows a command (`EQbCommandStates`).
    pub command_enabled: fn(u32) -> bool,
    /// `EQExecuteCmd(command, key_down, null, null)`.
    pub execute_cmd: fn(u32, bool) -> bool,
    pub face_angle: fn() -> f64,
    pub set_face_angle: fn(f64),
    /// The name MacroQuest gave a loaded plugin (see `mqemu_loaded_plugin_name`).
    pub loaded_plugin_name: fn(&str) -> Option<String>,
    /// MacroQuest's data system: TLOs and their members.
    pub data: crate::data::DataApi,
    /// The first ImGui error the shim caught since the last call, if any.
    pub take_imgui_error: fn() -> Option<String>,
    /// The ImGui, ImPlot and MacroQuest drawing shims' functions (see `mq_emu_sys::imgui::Api`).
    pub imgui: &'static crate::imgui::Apis,
}

/// The real MacroQuest host. Only referenced by `Runtime::new`, which only
/// plugins (through `plugin!`) call.
pub(crate) const fn real<P>() -> Host {
    Host {
        write_chat: real_write_chat::<P>,
        main_interface: real_main_interface::<P>,
        game_state: real_game_state::<P>,
        now: Instant::now,
        local_player: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pLocalPlayer) },
        controlled_player: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pControlledPlayer) },
        target: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pTarget) },
        spawn_manager: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pSpawnManager) },
        local_pc: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pLocalPC) },
        spell_manager: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pSpellMgr) },
        world_data: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pWorldData) },
        zone_info: || unsafe { mq_emu_sys::eqlib::pZoneInfo },
        ground_items: || unsafe { computed(&raw const mq_emu_sys::eqlib::pItemList) },
        everquest_info: || unsafe { mq_emu_sys::eqlib::pEverQuestInfo },
        target_buffs_populated: || unsafe { mq_emu_sys::mq2main::gTargetbuffs },
        target_window: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pTargetWnd) },
        pet_window: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pPetInfoWnd) },
        raid: || unsafe { mq_emu_sys::eqlib::pRaid },
        mercenary_manager: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pMercManager) },
        aggro: || unsafe { computed(&raw const mq_emu_sys::eqlib::pAggroInfo) },
        merchant_window: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pMerchantWnd) },
        loot_window: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pLootWnd) },
        active_merchant: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pActiveMerchant) },
        active_corpse: || unsafe { foreign(&raw const mq_emu_sys::eqlib::pActiveCorpse) },
        game_fns: &mq_emu_sys::game::API,
        command_enabled: real_command_enabled::<P>,
        execute_cmd: |command, down| unsafe { mq_emu_sys::eqlib::EQExecuteCmd(command, down, std::ptr::null_mut(), std::ptr::null()) },
        face_angle: || unsafe { mq_emu_sys::mq2main::gFaceAngle },
        set_face_angle: |value| unsafe { mq_emu_sys::mq2main::gFaceAngle = value },
        loaded_plugin_name: real_loaded_plugin_name::<P>,
        data: crate::data::real::<P>(),
        take_imgui_error: real_take_imgui_error::<P>,
        imgui: &crate::imgui::Apis {
            imgui: &mq_emu_sys::imgui::API,
            implot: &mq_emu_sys::implot::API,
            mq: &mq_emu_sys::imgui_mq::API,
        },
    }
}

/// Reads an eqlib `ForeignPointer` global.
///
/// # Safety
/// `global` must be one of eqlib's `ForeignPointer` globals.
unsafe fn foreign<T>(global: *const ForeignPointer<T>) -> *mut T {
    unsafe { (*global).get() }
}

/// Reads an eqlib `ComputedPointer` global.
///
/// # Safety
/// `global` must be one of eqlib's `ComputedPointer` globals.
unsafe fn computed<T>(global: *const ComputedPointer<T>) -> *mut T {
    unsafe { (*global).get() }
}

fn real_command_enabled<P>(command: u32) -> bool {
    let states = unsafe { mq_emu_sys::eqlib::EQbCommandStates };
    (command as usize) < N_EQ_MAPPABLE_COMMANDS && !states.is_null() && unsafe { *states.add(command as usize) } != 0
}

fn real_write_chat<P>(text: &str, color: i32) {
    let text = to_c(truncate(text, MAX_STRING - 1));
    unsafe { mq_emu_sys::mq2main::WriteChatColor(text.as_ptr(), color, 0) }
}

fn real_main_interface<P>() -> *mut MainInterface {
    unsafe { mq_emu_sys::shim::mqemu_main_interface() }
}

fn real_loaded_plugin_name<P>(name: &str) -> Option<String> {
    let name = to_c(name);
    let found = unsafe { mq_emu_sys::shim::mqemu_loaded_plugin_name(name.as_ptr()) };
    (!found.is_null()).then(|| unsafe { std::ffi::CStr::from_ptr(found) }.to_string_lossy().into_owned())
}

fn real_take_imgui_error<P>() -> Option<String> {
    let mut buffer = [0 as std::ffi::c_char; 512];
    unsafe { (mq_emu_sys::imgui::API.mqemu_imgui_take_error)(buffer.as_mut_ptr(), buffer.len()) }
        .then(|| crate::text::from_c_array(&buffer))
}

fn real_game_state<P>() -> i32 {
    unsafe { mq_emu_sys::mq2main::GetGameState() }
}

/// Cuts `text` to at most `max_bytes`, on a character boundary. MacroQuest
/// copies chat into `MAX_STRING` buffers.
pub(crate) fn truncate(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdef", 3), "abc");
        // 'é' is two bytes; cutting through it backs off to before it.
        assert_eq!(truncate("caf\u{e9}", 4), "caf");
        assert_eq!(truncate("", 0), "");
    }
}
