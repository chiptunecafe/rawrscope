use crate::state::State;

use bitflags::bitflags;
use imgui::{im_str, Ui};
use tinyfiledialogs as tfd;

bitflags! {
    #[derive(Default)]
    pub struct ExternalEvents: u32 {
        const REBUILD_MASTER = 0b00000001;
        const REDRAW_SCOPES = 0b00000010;
    }
}

pub fn ui<'a, 'ui>(state: &'a mut State, ui: &'a Ui<'ui>, ext_events: &'a mut ExternalEvents) {
    ui.main_menu_bar(|| {
        ui.menu(im_str!("File"), true, || {
            if imgui::MenuItem::new(&im_str!("Open")).build(ui) {
                if let Some(path) = tfd::open_file_dialog(
                    "Open Project...",
                    ".",
                    Some((&["*.rprj"], "rawrscope projects")),
                ) {
                    // TODO do not panic and log warnings
                    *state = State::from_file(path).expect("could not load project").0;
                    *ext_events |= ExternalEvents::REBUILD_MASTER;
                }
            }

            if imgui::MenuItem::new(&im_str!("Save"))
                .enabled(state.file_path.as_os_str().len() != 0)
                .build(ui)
            {
                // TODO do not panic
                state
                    .write(&state.file_path)
                    .expect("could not save project");
            }
        });
    });

    imgui::Window::new(&im_str!(
        "rawrscope {} ({})",
        clap::crate_version!(),
        git_version::git_version!()
    ))
    .size([300.0, 90.0], imgui::Condition::Always)
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
    });

    imgui::Window::new(im_str!("Experimental Options"))
        .size([250.0, 190.0], imgui::Condition::Always)
        .resizable(false)
        .build(&ui, || {
            ui.checkbox(
                im_str!("Multithreaded Centering"),
                &mut state.debug.multithreaded_centering,
            );
            ui.checkbox(im_str!("Stutter Test"), &mut state.debug.stutter_test);

            let (ft_left, ft_right) = state.debug.frametimes.as_slices();
            let frametimes = [ft_left, ft_right].concat();
            imgui::PlotLines::new(&ui, &im_str!(""), &frametimes)
                .scale_min(0.0)
                .scale_max(20.0)
                .graph_size([234.0, 100.0])
                .overlay_text(&im_str!("Frametime (0-20ms)"))
                .build();
        });
}
