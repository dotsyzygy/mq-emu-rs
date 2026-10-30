//! What another UI library drawn in an ImGui window needs: textures from
//! memory, meshes, and input as ImGui took it.

use super::*;

/// A texture's status, queued updates, their bounding box and the pixel at `at`.
fn state(texture: &UserTexture, at: [i32; 2]) -> (i32, i32, [i32; 4], u32) {
    let (mut status, mut updates, mut rect, mut pixel) = (0, 0, [0; 4], 0);
    unsafe { mqemu_imgui_test_texture_state(texture.raw(), at[0], at[1], &mut status, &mut updates, rect.as_mut_ptr(), &mut pixel) };
    (status, updates, rect, pixel)
}

fn render(texture: &UserTexture) {
    unsafe { mqemu_imgui_test_render_texture(texture.raw()) };
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 128];

#[test]
fn a_user_texture_uploads_updates_and_comes_back_after_the_device_is_lost() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let registered = unsafe { mqemu_imgui_test_user_textures() };
    let mut texture = ui.create_texture([3, 2], &[RED, RED, RED, RED, GREEN, RED]);
    assert_eq!(unsafe { mqemu_imgui_test_user_textures() }, registered + 1);
    // Pixels as given, RGBA with red lowest (IM_COL32), row by row.
    assert_eq!(state(&texture, [1, 1]), (sys::ImTextureStatus_WantCreate, 0, [0xffff, 0xffff, 0, 0], u32::from_le_bytes(GREEN)));
    assert!(!texture.is_uploaded());

    render(&texture);
    assert!(texture.is_uploaded());
    assert_eq!(state(&texture, [0, 0]).0, sys::ImTextureStatus_OK);

    // Updates queue rectangles and their bounding box for the renderer.
    texture.update([2, 0], [1, 1], &[GREEN]);
    texture.update([0, 1], [2, 1], &[GREEN, GREEN]);
    let (status, updates, rect, pixel) = state(&texture, [2, 0]);
    assert_eq!((status, updates, rect, pixel), (sys::ImTextureStatus_WantUpdates, 2, [0, 0, 3, 2], u32::from_le_bytes(GREEN)));
    render(&texture);
    // The renderer leaves the last ones listed: the next update starts afresh.
    texture.update([1, 0], [1, 1], &[RED]);
    let (status, updates, rect, _) = state(&texture, [1, 0]);
    assert_eq!((status, updates, rect), (sys::ImTextureStatus_WantUpdates, 1, [1, 0, 1, 1]));
    render(&texture);

    // A lost device frees it; ImGui asks for it again, with its pixels.
    let released = unsafe { mqemu_imgui_test_released_textures() };
    unsafe { mqemu_imgui_test_lose_device(texture.raw()) };
    assert_eq!(unsafe { mqemu_imgui_test_released_textures() }, released + 1);
    assert_eq!(state(&texture, [0, 1]), (sys::ImTextureStatus_WantCreate, 1, [1, 0, 1, 1], u32::from_le_bytes(GREEN)));
    // An update before it's uploaded again goes with the whole.
    texture.update([0, 0], [1, 1], &[GREEN]);
    assert_eq!(state(&texture, [0, 0]).0, sys::ImTextureStatus_WantCreate);
    render(&texture);

    // Dropped, it stays until the next frame starts (the last may still draw it).
    drop(texture);
    assert_eq!(unsafe { mqemu_imgui_test_user_textures() }, registered + 1);
    free_retired_textures();
    assert_eq!(unsafe { mqemu_imgui_test_user_textures() }, registered);
    assert_eq!(unsafe { mqemu_imgui_test_released_textures() }, released + 2, "freeing released the renderer's texture");
}

#[test]
fn a_texture_never_uploaded_frees_without_a_renderer_texture() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let released = unsafe { mqemu_imgui_test_released_textures() };
    drop(ui.create_texture([1, 1], &[RED]));
    free_retired_textures();
    assert_eq!(unsafe { mqemu_imgui_test_released_textures() }, released);
}

#[test]
fn bad_texture_sizes_and_rectangles_panic_before_reaching_imgui() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let registered = unsafe { mqemu_imgui_test_user_textures() };
    let panics = |f: &dyn Fn()| catch_unwind(AssertUnwindSafe(f)).is_err();
    assert!(panics(&|| drop(ui.create_texture([0, 1], &[]))));
    assert!(panics(&|| drop(ui.create_texture([2, 2], &[RED; 3]))));
    assert_eq!(unsafe { mqemu_imgui_test_user_textures() }, registered);
    let mut texture = ui.create_texture([2, 2], &[RED; 4]);
    for (pos, size, n) in [([1, 1], [2, 1], 2), ([0, 0], [1, 1], 2), ([usize::MAX, 0], [2, 1], 2)] {
        assert!(catch_unwind(AssertUnwindSafe(|| texture.update(pos, size, &vec![RED; n]))).is_err(), "{pos:?} {size:?}");
    }
    texture.update([1, 1], [0, 0], &[]);
    drop(texture);
    free_retired_textures();
}

#[test]
fn a_mesh_draws_its_triangles_with_its_texture_and_clip() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let texture = ui.create_texture([1, 1], &[RED]);
    let quad = [[100.0, 100.0], [140.0, 100.0], [140.0, 140.0], [100.0, 140.0]];
    let vertices: Vec<DrawVert> = quad.iter().map(|&pos| DrawVert { pos, uv: [0.5, 0.5], color: 0x8000_00ff }).collect();
    let indices = [0, 1, 2, 0, 2, 3];
    frame(|ui| {
        test_window(ui, |ui| {
            let draw = ui.draw_list();
            // Something before it, so its indices must be offset.
            draw.add_rect_filled([20.0, 20.0], [30.0, 30.0], [1.0; 4], 0.0);
            let (vertices_before, indices_before) = (vertices_in(&draw), unsafe { mqemu_imgui_test_list_indices(draw.raw_list()) });
            let clip_depth_before = clip_depth(&draw);
            draw.add_mesh(&texture, &vertices, &indices, [[110.0, 0.0], [2000.0, 2000.0]]);
            assert_eq!(vertices_in(&draw), vertices_before + 4);
            let written: Vec<i32> = (0..6).map(|i| unsafe { mqemu_imgui_test_list_index(draw.raw_list(), indices_before + i) }).collect();
            assert_eq!(written, indices.map(|i| vertices_before + i32::from(i)));
            let mut vertex = sys::ImDrawVert::default();
            unsafe { mqemu_imgui_test_list_vertex(draw.raw_list(), vertices_before + 2, &mut vertex) };
            assert_eq!(vertex, sys::ImDrawVert { pos: [140.0, 140.0], uv: [0.5, 0.5], col: 0x8000_00ff });
            let (mut clip, mut count) = ([0.0; 4], 0);
            assert!(unsafe { mqemu_imgui_test_list_texture_cmd(draw.raw_list(), texture.raw(), clip.as_mut_ptr(), &mut count) });
            assert_eq!(count, 6);
            // Our clip within the window's: left from ours, the rest from the window's.
            let (window_min, size) = (ui.window_pos(), ui.window_size());
            let window_max = [window_min[0] + size[0], window_min[1] + size[1]];
            assert_eq!(clip[0], 110.0);
            assert!(clip[1] >= window_min[1] && clip[2] <= window_max[0] && clip[3] <= window_max[1], "{clip:?}");
            // Nothing for nothing.
            draw.add_mesh(&texture, &[], &[], [[0.0, 0.0], [10.0, 10.0]]);
            assert_eq!(vertices_in(&draw), vertices_before + 4);
            assert_eq!(clip_depth(&draw), clip_depth_before, "the mesh's clip rectangle was popped");
        });
    })
    .unwrap();
    drop(texture);
    free_retired_textures();
}

fn clip_depth(draw: &DrawList<'_>) -> i32 {
    let (mut depth, mut path, mut channels) = (0, 0, 0);
    unsafe { mqemu_imgui_test_list_state(draw.raw_list(), &mut depth, &mut path, &mut channels) };
    depth
}

fn vertices_in(draw: &DrawList<'_>) -> i32 {
    unsafe { mqemu_imgui_test_list_vertices(draw.raw_list()) }
}

#[test]
fn bad_meshes_panic_before_reaching_imgui() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let texture = ui.create_texture([1, 1], &[RED]);
    let three = [DrawVert::default(); 3];
    frame(|ui| {
        test_window(ui, |ui| {
            let draw = ui.draw_list();
            let before = vertices_in(&draw);
            let clip = [[0.0, 0.0], [100.0, 100.0]];
            let panics = |f: &dyn Fn()| catch_unwind(AssertUnwindSafe(f)).is_err();
            assert!(panics(&|| draw.add_mesh(&texture, &three, &[0, 1, 3], clip)), "an index past the vertices");
            assert!(panics(&|| draw.add_mesh(&texture, &three, &[0, 1], clip)), "not whole triangles");
            assert!(panics(&|| draw.add_mesh(&texture, &vec![DrawVert::default(); (1 << 16) + 1], &[0, 1, 2], clip)), "too many vertices");
            assert_eq!(vertices_in(&draw), before);
            // 65536 vertices is fine, past what earlier ones used: ImGui starts a new vertex offset.
            let many = vec![DrawVert::default(); 1 << 16];
            draw.add_mesh(&texture, &many, &[0, 1, 65535], clip);
            draw.add_mesh(&texture, &many, &[0, 1, 65535], clip);
        });
    })
    .unwrap();
    drop(texture);
    free_retired_textures();
}

#[test]
fn input_events_come_as_imgui_took_them() {
    let _imgui = lock();
    frame(|_| {}).unwrap();
    unsafe {
        mqemu_imgui_test_wheel(0.0, -1.0);
        mqemu_imgui_test_modifiers(true, false);
        mqemu_imgui_test_letter_key(b'C' as c_char, true);
        mqemu_imgui_test_type(c"é".as_ptr());
    }
    // ImGui trickles them: what conflicts with an earlier event waits a frame.
    let (mut events, mods) = frame_at([50.0, 60.0], true, |ui| (ui.input_events(), ui.key_mods())).unwrap();
    events.extend(frame_at([50.0, 60.0], true, |ui| ui.input_events()).unwrap());
    for expected in [
        InputEvent::Key { key: KeyCode(sys::ImGuiMod_Ctrl), down: true },
        InputEvent::MouseMoved([50.0, 60.0]),
        InputEvent::MouseButton { button: 0, down: true },
        InputEvent::MouseWheel([0.0, -1.0]),
        InputEvent::Key { key: KeyCode::C, down: true },
        InputEvent::Text('é'),
    ] {
        assert!(events.contains(&expected), "{expected:?} not in {events:?}");
    }
    assert_eq!(mods, KeyMods::CTRL);
    // Taken once: the next frame has only what came after.
    unsafe {
        mqemu_imgui_test_letter_key(b'C' as c_char, false);
        mqemu_imgui_test_modifiers(false, false);
    }
    let mut events = frame_at([50.0, 60.0], false, |ui| ui.input_events()).unwrap();
    events.extend(frame_at([50.0, 60.0], false, |ui| ui.input_events()).unwrap());
    assert!(events.contains(&InputEvent::Key { key: KeyCode::C, down: false }), "{events:?}");
    assert!(!events.contains(&InputEvent::Text('é')) && !events.contains(&InputEvent::MouseWheel([0.0, -1.0])), "{events:?}");
}

/// What MacroQuest does in game: clicks and the wheel go straight into
/// ImGui's state, so they're no input events: read the state instead.
#[test]
fn macroquest_s_clicks_and_wheel_are_state_not_events() {
    let _imgui = lock();
    let as_mq = |down: bool, wheel: f32| {
        assert!(unsafe { mqemu_imgui_test_new_frame_as_mq(50.0, 60.0, down, wheel) });
        let ui = Ui::new(&fake::APIS);
        let seen = (ui.input_events(), ui.is_mouse_down(MouseButton::Left), ui.mouse_wheel());
        let mut error = [0 as c_char; 512];
        assert!(unsafe { mqemu_imgui_test_end_frame(error.as_mut_ptr(), error.len()) });
        seen
    };
    as_mq(false, 0.0);
    let (events, down, wheel) = as_mq(true, -2.0);
    assert!(down && wheel == [0.0, -2.0]);
    assert!(!events.iter().any(|e| matches!(e, InputEvent::MouseButton { .. } | InputEvent::MouseWheel(_))), "{events:?}");
    // The wheel lasts one frame.
    let (_, down, wheel) = as_mq(false, 0.0);
    assert!(!down && wheel == [0.0, 0.0]);
}

#[test]
fn a_held_key_repeats() {
    let _imgui = lock();
    frame(|_| {}).unwrap();
    unsafe { mqemu_imgui_test_letter_key(b'K' as c_char, true) };
    let first = frame(|ui| (ui.is_key_pressed(KeyCode::K, false), ui.is_key_down(KeyCode::K), ui.is_key_down(KeyCode::J))).unwrap();
    assert_eq!(first, (true, true, false));
    // Held: no new press without repeat; repeats once ImGui's delay passes (60 fps frames).
    let mut repeats = 0;
    for _ in 0..60 {
        let (pressed, repeated) = frame(|ui| (ui.is_key_pressed(KeyCode::K, false), ui.is_key_pressed(KeyCode::K, true))).unwrap();
        assert!(!pressed);
        repeats += i32::from(repeated);
    }
    assert!(repeats > 3, "{repeats} repeats in a second");
    unsafe { mqemu_imgui_test_letter_key(b'K' as c_char, false) };
    assert_eq!(frame(|ui| (ui.is_key_released(KeyCode::K), ui.is_key_down(KeyCode::K))).unwrap(), (true, false));
    // Keys ImGui doesn't know are never pressed (rather than an ImGui error).
    for key in [KeyCode::NONE, KeyCode(5), KeyCode(sys::ImGuiKey_NamedKey_END), KeyCode(sys::ImGuiMod_Ctrl | sys::ImGuiMod_Shift)] {
        assert!(!key.is_valid());
        frame(|ui| assert!(!ui.is_key_pressed(key, true) && !ui.is_key_down(key) && !ui.is_key_released(key))).unwrap();
    }
    assert!(KeyCode(sys::ImGuiMod_Ctrl).is_valid() && KeyCode::TAB.is_valid());
    assert!(take_error().is_none());
}

#[test]
fn capturing_the_keyboard_and_mouse_holds_for_the_next_frame() {
    let _imgui = lock();
    let want = || {
        let (mut keyboard, mut mouse) = (false, false);
        unsafe { mqemu_imgui_test_want_capture(&mut keyboard, &mut mouse) };
        (keyboard, mouse)
    };
    frame_at([2000.0, 2000.0], false, |ui| {
        assert_eq!(want(), (false, false));
        ui.set_next_frame_want_capture_keyboard(true);
        ui.set_next_frame_want_capture_mouse(true);
    })
    .unwrap();
    frame_at([2000.0, 2000.0], false, |_| assert_eq!(want(), (true, true))).unwrap();
    frame_at([2000.0, 2000.0], false, |_| assert_eq!(want(), (false, false))).unwrap();
}

#[test]
fn a_clicked_window_has_focus() {
    let _imgui = lock();
    // Two windows: clicking one focuses it.
    let focused = |at: [f32; 2], down: bool| {
        frame_at(at, down, |ui| {
            let other = ui.window("other").position([700.0, 10.0], Cond::Always).size([200.0, 200.0], Cond::Always);
            let other = other.flags(WindowFlags::NO_SAVED_SETTINGS).show(|ui| ui.is_window_focused(FocusedFlags::NONE)).unwrap();
            (test_window(ui, |ui| ui.is_window_focused(FocusedFlags::NONE)).unwrap(), other)
        })
        .unwrap()
    };
    // Hovering uses last frame's windows: both exist first.
    focused([0.0, 0.0], false);
    focused([800.0, 100.0], true);
    assert_eq!(focused([800.0, 100.0], false), (false, true));
    focused([20.0, 40.0], true);
    assert_eq!(focused([20.0, 40.0], false), (true, false));
}

#[test]
fn the_clipboard_round_trips() {
    let _imgui = lock();
    frame(|ui| {
        ui.set_clipboard_text("short");
        assert_eq!(ui.clipboard_text(), "short");
        let long = "Grüße ".repeat(100);
        ui.set_clipboard_text(&long);
        assert_eq!(ui.clipboard_text(), long);
    })
    .unwrap();
}
