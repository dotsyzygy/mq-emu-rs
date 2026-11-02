#[test]
fn color_macros_work_from_another_crate() {
    assert_eq!(mq_emu::blue!("hi"), "\x07uhi\x07x");
}
