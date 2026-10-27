//! The trade window: whether each side has pressed Trade, and what's offered.

use std::fmt;

use mq_emu_sys::constants::MAX_TRADE_SLOTS;
use mq_emu_sys::layouts::CTradeWnd;

use super::Window;
use super::item_slots::ItemSlotWindow;
use crate::items::ItemSlots;
use crate::mq::Mq;
use crate::plugin::Plugin;

/// The trade window (with a player or an NPC), for the duration of the
/// current callback. The game keeps it whether or not you're trading.
#[derive(Clone, Copy)]
pub struct TradeWindow<'a> {
    window: Window<'a>,
    trade: &'a CTradeWnd,
}

impl<'a> TradeWindow<'a> {
    pub fn window(&self) -> Window<'a> {
        self.window
    }

    pub fn raw(&self) -> &'a CTradeWnd {
        self.trade
    }

    /// Whether you're trading with someone.
    pub fn is_trading(&self) -> bool {
        self.trade.bIsTrading()
    }

    /// Whether you've pressed Trade (`${Window.MyTradeReady}`).
    pub fn my_ready(&self) -> bool {
        self.trade.bMyReadyTrade()
    }

    /// Whether the other trader has pressed Trade (`${Window.HisTradeReady}`).
    pub fn their_ready(&self) -> bool {
        self.trade.bHisReadyTrade()
    }

    /// What's offered, by trade slot: yours in slots 0 to 7, the other
    /// trader's after (`/itemnotify trade<slot+1>`).
    pub fn items(&self) -> ItemSlots<'a> {
        // SAFETY: the window's container lives as long as the window.
        unsafe { ItemSlots::from_raw(self.trade.TradeItems()) }
    }

    /// The item slot window of trade slot `slot`, from 0.
    pub fn slot_window(&self, slot: usize) -> Option<ItemSlotWindow<'a>> {
        let raw = if slot < MAX_TRADE_SLOTS { self.trade.pInvSlotWnd_at(slot) } else { std::ptr::null_mut() };
        self.window.related(raw.cast())?.as_item_slot()
    }
}

impl fmt::Debug for TradeWindow<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TradeWindow")
            .field("trading", &self.is_trading())
            .field("my_ready", &self.my_ready())
            .field("their_ready", &self.their_ready())
            .field("items", &self.items())
            .finish()
    }
}

impl<P: Plugin> Mq<P> {
    /// The trade window, once the game has made it.
    pub fn trade_window(&self) -> Option<TradeWindow<'_>> {
        let raw = (self.host().trade_window)();
        let window = unsafe { Window::from_raw(raw.cast(), self.window_api()) }?;
        Some(TradeWindow { window, trade: window.cast::<CTradeWnd>() })
    }
}
