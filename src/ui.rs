use crate::state::State;

use bitflags::bitflags;
use imgui::{im_str, Ui};
use tinyfiledialogs as tfd;

use crate::scope::centering;

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

fn ms_slider(label: &str, value: &mut f32, ui: &imgui::Ui) {
    let mut ms = *value * 1000.;
    imgui::DragFloat::new(ui, &im_str!("{}", label), &mut ms)
        .min(1.0)
        .max(5000.0)
        .display_format(&im_str!("%.2f ms"))
        .build();
    *value = ms / 1000.;
}

fn scope_editor(name: &str, scope: &mut crate::scope::Scope, ui: &imgui::Ui) {
    if imgui::CollapsingHeader::new(&im_str!("{}", name)).build(ui) {
        let im_id = ui.push_id(name);

        ui.text("Appearance");
        ms_slider("Window Size", &mut scope.window_size, ui);

        // TODO better ui
        let mut pos = [scope.rect.x as i32, scope.rect.y as i32];
        imgui::DragInt2::new(ui, &im_str!("Top Left"), &mut pos)
            .speed(0.1)
            .build();
        scope.rect.x = pos[0] as u32;
        scope.rect.y = pos[1] as u32;
        let mut size = [scope.rect.w as i32, scope.rect.h as i32];
        imgui::DragInt2::new(ui, &im_str!("Bottom Right"), &mut size)
            .speed(0.1)
            .build();
        scope.rect.w = size[0] as u32;
        scope.rect.h = size[1] as u32;

        ui.spacing();

        ui.text("Style");
        imgui::DragFloat::new(ui, &im_str!("Line Width"), &mut scope.line_width)
            .min(0.0)
            .max(100.0)
            .speed(0.25)
            .display_format(&im_str!("%.2f px"))
            .build();

        ui.spacing();

        ui.text("Centering");
        ms_slider("Trigger Width", &mut scope.trigger_width, ui);
        imgui::ComboBox::new(&im_str!("Algorithm"))
            .preview_value(&im_str!("{}", scope.centering))
            .build(ui, || {
                // TODO maybe generate this code
                if imgui::Selectable::new(&im_str!("None")).build(ui) {
                    scope.centering = centering::Centering::NoCentering(centering::NoCentering);
                }
                if imgui::Selectable::new(&im_str!("Zero Crossing")).build(ui) {
                    scope.centering = centering::Centering::ZeroCrossing(centering::ZeroCrossing);
                }
            });

        im_id.pop(ui);
    }
}

pub fn ui<'a, 'ui>(state: &'a mut State, ui: &'a Ui<'ui>, ext_events: &'a mut ExternalEvents) {
    // hack
    *ext_events |= ExternalEvents::REDRAW_SCOPES;

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
            view_toggle(&mut state.ui.show_scopes, im_str!("Scope Properties"), ui);
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
    let scopes = &mut state.scopes;

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

    if uistate.show_scopes {
        imgui::Window::new(im_str!("Scope Properties"))
            .size([320.0, 400.0], imgui::Condition::Always)
            .resizable(false) // TODO make resizable
            .opened(&mut uistate.show_scopes)
            .build(&ui, || {
                for (name, scope) in scopes.iter_mut() {
                    scope_editor(name, scope, ui);
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
