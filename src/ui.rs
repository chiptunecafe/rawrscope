use crate::state::State;
use imgui::{im_str, Ui};

pub fn ui<'a, 'ui>(state: &'a mut State, ui: &'a Ui<'ui>) {
    imgui::Window::new(im_str!("Rawrscope"))
        .size([300.0, 100.0], imgui::Condition::Always)
        .resizable(false)
        .build(&ui, || {
            ui.text(im_str!("Playing: {}", state.playback.playing));
            ui.text(im_str!("Frame: {}", state.playback.frame));
            if ui.small_button(im_str!("Play/Pause")) {
                state.playback.playing = !state.playback.playing;
            }
            ui.same_line(0.0);
            if ui.small_button(im_str!("+100 frames")) {
                state.playback.frame = state.playback.frame.saturating_add(100);
            }
            ui.same_line(0.0);
            if ui.small_button(im_str!("-100 frames")) {
                state.playback.frame = state.playback.frame.saturating_sub(100);
            }
        });
}
