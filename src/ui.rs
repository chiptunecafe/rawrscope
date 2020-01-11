use crate::state::State;
use imgui::{im_str, Ui};

pub fn ui<'a, 'ui>(state: &'a mut State, ui: &'a Ui<'ui>) {
    imgui::Window::new(&im_str!(
        "rawrscope {} ({})",
        clap::crate_version!(),
        git_version::git_version!()
    ))
    .size([300.0, 120.0], imgui::Condition::Always)
    .resizable(false)
    .build(&ui, || {
        // Status
        ui.text(im_str!("Playing: {}", state.playback.playing));
        ui.text(im_str!("Frame: {}", state.playback.frame));
        ui.text(im_str!("CPU Time: {:?}", state.debug.frametime));

        // Playback controls
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

        ui.separator();

        // Save project
        if ui.small_button(im_str!("Save project")) {
            // TODO do not panic
            state
                .write(&state.file_path)
                .expect("could not save project");
        }
    });

    imgui::Window::new(im_str!("Experimental Options"))
        .size([250.0, 100.0], imgui::Condition::Always)
        .resizable(false)
        .build(&ui, || {
            ui.checkbox(
                im_str!("Multithreaded Centering"),
                &mut state.debug.multithreaded_centering,
            );
            ui.checkbox(im_str!("Stutter Test"), &mut state.debug.stutter_test);
        });
}
