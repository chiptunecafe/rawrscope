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

    imgui::Window::new(im_str!("Debug Tools"))
        .size([200.0, 80.0], imgui::Condition::Always)
        .resizable(false)
        .build(&ui, || {
            imgui::Slider::new(im_str!("lag"), 0.0..=50.0).build(&ui, &mut state.debug.sleep);
            ui.checkbox(im_str!("Stutter Test"), &mut state.debug.stutter_test);
        });
    if state.debug.sleep > 0f32 {
        std::thread::sleep(std::time::Duration::from_secs_f32(
            state.debug.sleep / 1000.0,
        ));
    }
}
