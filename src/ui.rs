use crate::state::State;
use imgui::{im_str, Ui};

pub fn ui<'a, 'ui>(state: &'a mut State, ui: &'a Ui<'ui>) {
    imgui::Window::new(im_str!("Rawrscope"))
        .size([300.0, 100.0], imgui::Condition::Always)
        .resizable(false)
        .build(&ui, || {
            ui.text(im_str!("Playing: {}", state.playing));
            ui.text(im_str!("Frame: {}", state.frame));
        });
}
