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

fn view_toggle(v: &mut bool, text: &imgui::ImStr, ui: &imgui::Ui) {
    if imgui::MenuItem::new(text).selected(*v).build(ui) {
        *v = !*v;
    }
}

pub fn ui<'a, 'ui>(state: &'a mut State, ui: &'a Ui<'ui>, ext_events: &'a mut ExternalEvents) {
    ui.main_menu_bar(|| {
        ui.menu(im_str!("File"), true, || {
            if imgui::MenuItem::new(im_str!("Open")).build(ui) {
                if let Some(path) = tfd::open_file_dialog(
                    "Open Project...",
                    ".",
                    Some((&["*.rprj"], "rawrscope projects")),
                ) {
                    // TODO do not panic and log warnings
                    *state = State::from_file(path).expect("could not load project").0;
                    *ext_events |= ExternalEvents::REBUILD_MASTER | ExternalEvents::REDRAW_SCOPES;
                }
            }

            if imgui::MenuItem::new(im_str!("Save"))
                .enabled(!state.file_path.as_os_str().is_empty())
                .build(ui)
            {
                // TODO do not panic
                state
                    .write(&state.file_path)
                    .expect("could not save project");
            }
        });
        ui.menu(im_str!("View"), true, || {
            view_toggle(&mut state.ui.show_main, im_str!("Main Window"), ui);
            view_toggle(
                &mut state.ui.show_debug,
                im_str!("Experimental Options"),
                ui,
            );
        });
    });

    // FIXME fun borrow checker workaround, should probably not have one
    // unified state struct anymore
    let uistate = &mut state.ui;
    let playstate = &mut state.playback;
    let dbgstate = &mut state.debug;

    if uistate.show_main {
        imgui::Window::new(&im_str!(
            "rawrscope {} ({})",
            clap::crate_version!(),
            git_version::git_version!()
        ))
        .size([300.0, 90.0], imgui::Condition::Always)
        .resizable(false)
        .opened(&mut uistate.show_main)
        .build(&ui, || {
            // Status
            ui.text(im_str!("Playing: {}", playstate.playing));
            ui.text(im_str!("Frame: {}", playstate.frame));

            // Playback controls
            if ui.small_button(im_str!("Play/Pause")) {
                playstate.playing = !playstate.playing;
            }
            ui.same_line(0.0);
            if ui.small_button(im_str!("+100 frames")) {
                playstate.frame = playstate.frame.saturating_add(100);
                *ext_events |= ExternalEvents::REDRAW_SCOPES;
            }
            ui.same_line(0.0);
            if ui.small_button(im_str!("-100 frames")) {
                playstate.frame = playstate.frame.saturating_sub(100);
                *ext_events |= ExternalEvents::REDRAW_SCOPES;
            }
        });
    }

    if uistate.show_debug {
        imgui::Window::new(im_str!("Experimental Options"))
            .size([250.0, 190.0], imgui::Condition::Always)
            .resizable(false)
            .opened(&mut uistate.show_debug)
            .build(&ui, || {
                ui.checkbox(
                    im_str!("Multithreaded Centering"),
                    &mut dbgstate.multithreaded_centering,
                );
                ui.checkbox(im_str!("Stutter Test"), &mut dbgstate.stutter_test);

                let (ft_left, ft_right) = dbgstate.frametimes.as_slices();
                let frametimes = [ft_left, ft_right].concat();
                imgui::PlotLines::new(&ui, &im_str!(""), &frametimes)
                    .scale_min(0.0)
                    .scale_max(20.0)
                    .graph_size([234.0, 100.0])
                    .overlay_text(&im_str!("Frametime (0-20ms)"))
                    .build();
            });
    }
}
