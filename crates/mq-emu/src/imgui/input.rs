//! Input as ImGui took it in this frame: its events (mouse, keys, typed
//! text), the modifier keys, which window has focus, and the clipboard. For
//! feeding another UI library drawn in an ImGui window.

use mq_emu_sys::imgui as sys;

use super::{KeyMods, Ui, Vec2, s};

/// A key as ImGui names it (`ImGuiKey_`), from [`InputEvent::Key`]. Not the
/// game's [`Key`](crate::Key), which is a DirectInput scan code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode(pub i32);

impl KeyCode {
    /// Whether ImGui knows it: a named key (not [`KeyCode::NONE`]) or one of
    /// [`KeyMods`]' single modifiers. ImGui asks about no others.
    pub fn is_valid(self) -> bool {
        (sys::ImGuiKey_NamedKey_BEGIN..sys::ImGuiKey_NamedKey_END).contains(&self.0)
            || [sys::ImGuiMod_Ctrl, sys::ImGuiMod_Shift, sys::ImGuiMod_Alt, sys::ImGuiMod_Super].contains(&self.0)
    }

    pub const NONE: KeyCode = KeyCode(sys::ImGuiKey_None);
    pub const TAB: KeyCode = KeyCode(sys::ImGuiKey_Tab);
    pub const LEFT_ARROW: KeyCode = KeyCode(sys::ImGuiKey_LeftArrow);
    pub const RIGHT_ARROW: KeyCode = KeyCode(sys::ImGuiKey_RightArrow);
    pub const UP_ARROW: KeyCode = KeyCode(sys::ImGuiKey_UpArrow);
    pub const DOWN_ARROW: KeyCode = KeyCode(sys::ImGuiKey_DownArrow);
    pub const PAGE_UP: KeyCode = KeyCode(sys::ImGuiKey_PageUp);
    pub const PAGE_DOWN: KeyCode = KeyCode(sys::ImGuiKey_PageDown);
    pub const HOME: KeyCode = KeyCode(sys::ImGuiKey_Home);
    pub const END: KeyCode = KeyCode(sys::ImGuiKey_End);
    pub const INSERT: KeyCode = KeyCode(sys::ImGuiKey_Insert);
    pub const DELETE: KeyCode = KeyCode(sys::ImGuiKey_Delete);
    pub const BACKSPACE: KeyCode = KeyCode(sys::ImGuiKey_Backspace);
    pub const SPACE: KeyCode = KeyCode(sys::ImGuiKey_Space);
    pub const ENTER: KeyCode = KeyCode(sys::ImGuiKey_Enter);
    pub const ESCAPE: KeyCode = KeyCode(sys::ImGuiKey_Escape);
    pub const LEFT_CTRL: KeyCode = KeyCode(sys::ImGuiKey_LeftCtrl);
    pub const LEFT_SHIFT: KeyCode = KeyCode(sys::ImGuiKey_LeftShift);
    pub const LEFT_ALT: KeyCode = KeyCode(sys::ImGuiKey_LeftAlt);
    pub const LEFT_SUPER: KeyCode = KeyCode(sys::ImGuiKey_LeftSuper);
    pub const RIGHT_CTRL: KeyCode = KeyCode(sys::ImGuiKey_RightCtrl);
    pub const RIGHT_SHIFT: KeyCode = KeyCode(sys::ImGuiKey_RightShift);
    pub const RIGHT_ALT: KeyCode = KeyCode(sys::ImGuiKey_RightAlt);
    pub const RIGHT_SUPER: KeyCode = KeyCode(sys::ImGuiKey_RightSuper);
    pub const MENU: KeyCode = KeyCode(sys::ImGuiKey_Menu);
    pub const KEY_0: KeyCode = KeyCode(sys::ImGuiKey_0);
    pub const KEY_1: KeyCode = KeyCode(sys::ImGuiKey_1);
    pub const KEY_2: KeyCode = KeyCode(sys::ImGuiKey_2);
    pub const KEY_3: KeyCode = KeyCode(sys::ImGuiKey_3);
    pub const KEY_4: KeyCode = KeyCode(sys::ImGuiKey_4);
    pub const KEY_5: KeyCode = KeyCode(sys::ImGuiKey_5);
    pub const KEY_6: KeyCode = KeyCode(sys::ImGuiKey_6);
    pub const KEY_7: KeyCode = KeyCode(sys::ImGuiKey_7);
    pub const KEY_8: KeyCode = KeyCode(sys::ImGuiKey_8);
    pub const KEY_9: KeyCode = KeyCode(sys::ImGuiKey_9);
    pub const A: KeyCode = KeyCode(sys::ImGuiKey_A);
    pub const B: KeyCode = KeyCode(sys::ImGuiKey_B);
    pub const C: KeyCode = KeyCode(sys::ImGuiKey_C);
    pub const D: KeyCode = KeyCode(sys::ImGuiKey_D);
    pub const E: KeyCode = KeyCode(sys::ImGuiKey_E);
    pub const F: KeyCode = KeyCode(sys::ImGuiKey_F);
    pub const G: KeyCode = KeyCode(sys::ImGuiKey_G);
    pub const H: KeyCode = KeyCode(sys::ImGuiKey_H);
    pub const I: KeyCode = KeyCode(sys::ImGuiKey_I);
    pub const J: KeyCode = KeyCode(sys::ImGuiKey_J);
    pub const K: KeyCode = KeyCode(sys::ImGuiKey_K);
    pub const L: KeyCode = KeyCode(sys::ImGuiKey_L);
    pub const M: KeyCode = KeyCode(sys::ImGuiKey_M);
    pub const N: KeyCode = KeyCode(sys::ImGuiKey_N);
    pub const O: KeyCode = KeyCode(sys::ImGuiKey_O);
    pub const P: KeyCode = KeyCode(sys::ImGuiKey_P);
    pub const Q: KeyCode = KeyCode(sys::ImGuiKey_Q);
    pub const R: KeyCode = KeyCode(sys::ImGuiKey_R);
    pub const S: KeyCode = KeyCode(sys::ImGuiKey_S);
    pub const T: KeyCode = KeyCode(sys::ImGuiKey_T);
    pub const U: KeyCode = KeyCode(sys::ImGuiKey_U);
    pub const V: KeyCode = KeyCode(sys::ImGuiKey_V);
    pub const W: KeyCode = KeyCode(sys::ImGuiKey_W);
    pub const X: KeyCode = KeyCode(sys::ImGuiKey_X);
    pub const Y: KeyCode = KeyCode(sys::ImGuiKey_Y);
    pub const Z: KeyCode = KeyCode(sys::ImGuiKey_Z);
    pub const F1: KeyCode = KeyCode(sys::ImGuiKey_F1);
    pub const F2: KeyCode = KeyCode(sys::ImGuiKey_F2);
    pub const F3: KeyCode = KeyCode(sys::ImGuiKey_F3);
    pub const F4: KeyCode = KeyCode(sys::ImGuiKey_F4);
    pub const F5: KeyCode = KeyCode(sys::ImGuiKey_F5);
    pub const F6: KeyCode = KeyCode(sys::ImGuiKey_F6);
    pub const F7: KeyCode = KeyCode(sys::ImGuiKey_F7);
    pub const F8: KeyCode = KeyCode(sys::ImGuiKey_F8);
    pub const F9: KeyCode = KeyCode(sys::ImGuiKey_F9);
    pub const F10: KeyCode = KeyCode(sys::ImGuiKey_F10);
    pub const F11: KeyCode = KeyCode(sys::ImGuiKey_F11);
    pub const F12: KeyCode = KeyCode(sys::ImGuiKey_F12);
    pub const F13: KeyCode = KeyCode(sys::ImGuiKey_F13);
    pub const F14: KeyCode = KeyCode(sys::ImGuiKey_F14);
    pub const F15: KeyCode = KeyCode(sys::ImGuiKey_F15);
    pub const F16: KeyCode = KeyCode(sys::ImGuiKey_F16);
    pub const F17: KeyCode = KeyCode(sys::ImGuiKey_F17);
    pub const F18: KeyCode = KeyCode(sys::ImGuiKey_F18);
    pub const F19: KeyCode = KeyCode(sys::ImGuiKey_F19);
    pub const F20: KeyCode = KeyCode(sys::ImGuiKey_F20);
    pub const F21: KeyCode = KeyCode(sys::ImGuiKey_F21);
    pub const F22: KeyCode = KeyCode(sys::ImGuiKey_F22);
    pub const F23: KeyCode = KeyCode(sys::ImGuiKey_F23);
    pub const F24: KeyCode = KeyCode(sys::ImGuiKey_F24);
    pub const APOSTROPHE: KeyCode = KeyCode(sys::ImGuiKey_Apostrophe);
    pub const COMMA: KeyCode = KeyCode(sys::ImGuiKey_Comma);
    pub const MINUS: KeyCode = KeyCode(sys::ImGuiKey_Minus);
    pub const PERIOD: KeyCode = KeyCode(sys::ImGuiKey_Period);
    pub const SLASH: KeyCode = KeyCode(sys::ImGuiKey_Slash);
    pub const SEMICOLON: KeyCode = KeyCode(sys::ImGuiKey_Semicolon);
    pub const EQUAL: KeyCode = KeyCode(sys::ImGuiKey_Equal);
    pub const LEFT_BRACKET: KeyCode = KeyCode(sys::ImGuiKey_LeftBracket);
    pub const BACKSLASH: KeyCode = KeyCode(sys::ImGuiKey_Backslash);
    pub const RIGHT_BRACKET: KeyCode = KeyCode(sys::ImGuiKey_RightBracket);
    pub const GRAVE_ACCENT: KeyCode = KeyCode(sys::ImGuiKey_GraveAccent);
    pub const CAPS_LOCK: KeyCode = KeyCode(sys::ImGuiKey_CapsLock);
    pub const SCROLL_LOCK: KeyCode = KeyCode(sys::ImGuiKey_ScrollLock);
    pub const NUM_LOCK: KeyCode = KeyCode(sys::ImGuiKey_NumLock);
    pub const PRINT_SCREEN: KeyCode = KeyCode(sys::ImGuiKey_PrintScreen);
    pub const PAUSE: KeyCode = KeyCode(sys::ImGuiKey_Pause);
    pub const KEYPAD0: KeyCode = KeyCode(sys::ImGuiKey_Keypad0);
    pub const KEYPAD1: KeyCode = KeyCode(sys::ImGuiKey_Keypad1);
    pub const KEYPAD2: KeyCode = KeyCode(sys::ImGuiKey_Keypad2);
    pub const KEYPAD3: KeyCode = KeyCode(sys::ImGuiKey_Keypad3);
    pub const KEYPAD4: KeyCode = KeyCode(sys::ImGuiKey_Keypad4);
    pub const KEYPAD5: KeyCode = KeyCode(sys::ImGuiKey_Keypad5);
    pub const KEYPAD6: KeyCode = KeyCode(sys::ImGuiKey_Keypad6);
    pub const KEYPAD7: KeyCode = KeyCode(sys::ImGuiKey_Keypad7);
    pub const KEYPAD8: KeyCode = KeyCode(sys::ImGuiKey_Keypad8);
    pub const KEYPAD9: KeyCode = KeyCode(sys::ImGuiKey_Keypad9);
    pub const KEYPAD_DECIMAL: KeyCode = KeyCode(sys::ImGuiKey_KeypadDecimal);
    pub const KEYPAD_DIVIDE: KeyCode = KeyCode(sys::ImGuiKey_KeypadDivide);
    pub const KEYPAD_MULTIPLY: KeyCode = KeyCode(sys::ImGuiKey_KeypadMultiply);
    pub const KEYPAD_SUBTRACT: KeyCode = KeyCode(sys::ImGuiKey_KeypadSubtract);
    pub const KEYPAD_ADD: KeyCode = KeyCode(sys::ImGuiKey_KeypadAdd);
    pub const KEYPAD_ENTER: KeyCode = KeyCode(sys::ImGuiKey_KeypadEnter);
    pub const KEYPAD_EQUAL: KeyCode = KeyCode(sys::ImGuiKey_KeypadEqual);
    pub const APP_BACK: KeyCode = KeyCode(sys::ImGuiKey_AppBack);
    pub const APP_FORWARD: KeyCode = KeyCode(sys::ImGuiKey_AppForward);
    pub const OEM102: KeyCode = KeyCode(sys::ImGuiKey_Oem102);
    pub const GAMEPAD_START: KeyCode = KeyCode(sys::ImGuiKey_GamepadStart);
    pub const GAMEPAD_BACK: KeyCode = KeyCode(sys::ImGuiKey_GamepadBack);
    pub const GAMEPAD_FACE_LEFT: KeyCode = KeyCode(sys::ImGuiKey_GamepadFaceLeft);
    pub const GAMEPAD_FACE_RIGHT: KeyCode = KeyCode(sys::ImGuiKey_GamepadFaceRight);
    pub const GAMEPAD_FACE_UP: KeyCode = KeyCode(sys::ImGuiKey_GamepadFaceUp);
    pub const GAMEPAD_FACE_DOWN: KeyCode = KeyCode(sys::ImGuiKey_GamepadFaceDown);
    pub const GAMEPAD_DPAD_LEFT: KeyCode = KeyCode(sys::ImGuiKey_GamepadDpadLeft);
    pub const GAMEPAD_DPAD_RIGHT: KeyCode = KeyCode(sys::ImGuiKey_GamepadDpadRight);
    pub const GAMEPAD_DPAD_UP: KeyCode = KeyCode(sys::ImGuiKey_GamepadDpadUp);
    pub const GAMEPAD_DPAD_DOWN: KeyCode = KeyCode(sys::ImGuiKey_GamepadDpadDown);
    pub const GAMEPAD_L1: KeyCode = KeyCode(sys::ImGuiKey_GamepadL1);
    pub const GAMEPAD_R1: KeyCode = KeyCode(sys::ImGuiKey_GamepadR1);
    pub const GAMEPAD_L2: KeyCode = KeyCode(sys::ImGuiKey_GamepadL2);
    pub const GAMEPAD_R2: KeyCode = KeyCode(sys::ImGuiKey_GamepadR2);
    pub const GAMEPAD_L3: KeyCode = KeyCode(sys::ImGuiKey_GamepadL3);
    pub const GAMEPAD_R3: KeyCode = KeyCode(sys::ImGuiKey_GamepadR3);
    pub const GAMEPAD_L_STICK_LEFT: KeyCode = KeyCode(sys::ImGuiKey_GamepadLStickLeft);
    pub const GAMEPAD_L_STICK_RIGHT: KeyCode = KeyCode(sys::ImGuiKey_GamepadLStickRight);
    pub const GAMEPAD_L_STICK_UP: KeyCode = KeyCode(sys::ImGuiKey_GamepadLStickUp);
    pub const GAMEPAD_L_STICK_DOWN: KeyCode = KeyCode(sys::ImGuiKey_GamepadLStickDown);
    pub const GAMEPAD_R_STICK_LEFT: KeyCode = KeyCode(sys::ImGuiKey_GamepadRStickLeft);
    pub const GAMEPAD_R_STICK_RIGHT: KeyCode = KeyCode(sys::ImGuiKey_GamepadRStickRight);
    pub const GAMEPAD_R_STICK_UP: KeyCode = KeyCode(sys::ImGuiKey_GamepadRStickUp);
    pub const GAMEPAD_R_STICK_DOWN: KeyCode = KeyCode(sys::ImGuiKey_GamepadRStickDown);
    pub const MOUSE_LEFT: KeyCode = KeyCode(sys::ImGuiKey_MouseLeft);
    pub const MOUSE_RIGHT: KeyCode = KeyCode(sys::ImGuiKey_MouseRight);
    pub const MOUSE_MIDDLE: KeyCode = KeyCode(sys::ImGuiKey_MouseMiddle);
    pub const MOUSE_X1: KeyCode = KeyCode(sys::ImGuiKey_MouseX1);
    pub const MOUSE_X2: KeyCode = KeyCode(sys::ImGuiKey_MouseX2);
    pub const MOUSE_WHEEL_X: KeyCode = KeyCode(sys::ImGuiKey_MouseWheelX);
    pub const MOUSE_WHEEL_Y: KeyCode = KeyCode(sys::ImGuiKey_MouseWheelY);
}

flags! {
    /// Which window [`Ui::is_window_focused`] asks about (`ImGuiFocusedFlags_`).
    FocusedFlags {
        /// Or one of its child windows.
        CHILD_WINDOWS = ImGuiFocusedFlags_ChildWindows,
        /// Its root window (the top of the window it's a child of).
        ROOT_WINDOW = ImGuiFocusedFlags_RootWindow,
        /// Any window.
        ANY_WINDOW = ImGuiFocusedFlags_AnyWindow,
        /// Not counting popups it opened.
        NO_POPUP_HIERARCHY = ImGuiFocusedFlags_NoPopupHierarchy,
        /// Counting windows docked into it.
        DOCK_HIERARCHY = ImGuiFocusedFlags_DockHierarchy,
        ROOT_AND_CHILD_WINDOWS = ImGuiFocusedFlags_RootAndChildWindows,
    }
}

/// One of the input events ImGui took in this frame ([`Ui::input_events`]),
/// whichever window they were over: the game passes ImGui its input whether
/// or not ImGui keeps it from the game.
///
/// In game, mouse clicks and the wheel are not among them: MacroQuest writes
/// those from DirectInput straight into ImGui's state. Read the mouse with
/// [`Ui::is_mouse_down`] and [`Ui::mouse_wheel`] instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    /// The mouse moved here, on the screen. `[f32::MIN; 2]` when it left
    /// the game's window.
    MouseMoved(Vec2),
    /// The wheel turned: `[x, y]` in notches, up and right positive.
    MouseWheel(Vec2),
    /// A mouse button (0 left, 1 right, 2 middle, 3 and 4 extra) went down
    /// or up.
    MouseButton { button: i32, down: bool },
    /// A key went down or up (repeats while held come as more downs).
    Key { key: KeyCode, down: bool },
    /// A character was typed.
    Text(char),
    /// The game's window gained or lost focus.
    Focus(bool),
}

impl InputEvent {
    fn from_raw(raw: &sys::InputEvent) -> Option<InputEvent> {
        Some(match raw.kind {
            sys::ImGuiInputEventType_MousePos => InputEvent::MouseMoved([raw.x, raw.y]),
            sys::ImGuiInputEventType_MouseWheel => InputEvent::MouseWheel([raw.x, raw.y]),
            sys::ImGuiInputEventType_MouseButton => InputEvent::MouseButton { button: raw.code, down: raw.down },
            sys::ImGuiInputEventType_Key => InputEvent::Key { key: KeyCode(raw.code), down: raw.down },
            sys::ImGuiInputEventType_Text => InputEvent::Text(char::from_u32(raw.code as u32)?),
            sys::ImGuiInputEventType_Focus => InputEvent::Focus(raw.down),
            _ => return None,
        })
    }
}

impl Ui {
    /// The input events ImGui took in this frame, oldest first.
    pub fn input_events(&self) -> Vec<InputEvent> {
        let count = unsafe { (self.api.mqemu_imgui_input_event_count)() };
        (0..count)
            .filter_map(|i| {
                let mut raw = sys::InputEvent::default();
                unsafe { (self.api.mqemu_imgui_input_event)(i, &mut raw) }.then_some(raw)
            })
            .filter_map(|raw| InputEvent::from_raw(&raw))
            .collect()
    }

    /// Whether `key` went down this frame; with `repeat`, also each time it
    /// repeats while held (at ImGui's key repeat rate). False for a key
    /// ImGui doesn't know ([`KeyCode::is_valid`]).
    pub fn is_key_pressed(&self, key: KeyCode, repeat: bool) -> bool {
        key.is_valid() && unsafe { (self.api.mqemu_imgui_is_key_pressed)(key.0, repeat) }
    }

    /// Whether `key` went up this frame.
    pub fn is_key_released(&self, key: KeyCode) -> bool {
        key.is_valid() && unsafe { (self.api.mqemu_imgui_is_key_released)(key.0) }
    }

    pub fn is_key_down(&self, key: KeyCode) -> bool {
        key.is_valid() && unsafe { (self.api.mqemu_imgui_is_key_down)(key.0) }
    }

    /// The modifier keys held now.
    pub fn key_mods(&self) -> KeyMods {
        KeyMods(unsafe { (self.api.mqemu_imgui_key_mods)() })
    }

    /// Whether the current window (or what `flags` ask about) has keyboard
    /// focus.
    pub fn is_window_focused(&self, flags: FocusedFlags) -> bool {
        unsafe { (self.api.mqemu_imgui_is_window_focused)(flags.bits()) }
    }

    /// Keeps the keyboard from the game next frame (or gives it back), what
    /// ImGui's own widgets want notwithstanding: for a text field ImGui
    /// doesn't know about.
    pub fn set_next_frame_want_capture_keyboard(&self, want: bool) {
        unsafe { (self.api.mqemu_imgui_set_next_frame_want_capture_keyboard)(want) }
    }

    /// Keeps the mouse from the game next frame (or gives it back).
    pub fn set_next_frame_want_capture_mouse(&self, want: bool) {
        unsafe { (self.api.mqemu_imgui_set_next_frame_want_capture_mouse)(want) }
    }

    /// The text on the clipboard ("" if none).
    pub fn clipboard_text(&self) -> String {
        let mut buffer = [0 as std::ffi::c_char; 256];
        let len = unsafe { (self.api.mqemu_imgui_clipboard_text)(buffer.as_mut_ptr(), buffer.len()) };
        if len < buffer.len() {
            return crate::text::from_c_array(&buffer);
        }
        let mut long = vec![0 as std::ffi::c_char; len + 1];
        unsafe { (self.api.mqemu_imgui_clipboard_text)(long.as_mut_ptr(), long.len()) };
        crate::text::from_c_array(&long)
    }

    pub fn set_clipboard_text(&self, text: &str) {
        let (p, n) = s(text);
        unsafe { (self.api.mqemu_imgui_set_clipboard_text)(p, n) }
    }
}
