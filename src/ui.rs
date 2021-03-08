use imgui::im_str;

// Non-serialized UI state
pub struct Ui {
    // Window visibility
    show_scope_window: bool,
}

impl Default for Ui {
    fn default() -> Self {
        Ui {
            show_scope_window: true,
        }
    }
}

impl Ui {
    fn scope_window(&mut self, ui: &imgui::Ui) {
        imgui::Window::new(im_str!("Scope Properties"))
            .opened(&mut self.show_scope_window)
            .size([300.0, 400.0], imgui::Condition::Always)
            .resizable(false)
            .build(ui, || {
                ui.small_button(im_str!("Add scope"));
                ui.same_line(0.0);
                ui.small_button(im_str!("Remove scope(s)"));

                imgui::ChildWindow::new("scopes")
                    .size([0.0, 100.0])
                    .border(true)
                    .build(ui, || {
                        imgui::Selectable::new(im_str!("ch1")).build(ui);
                        imgui::Selectable::new(im_str!("ch2")).build(ui);
                        imgui::Selectable::new(im_str!("ch3")).build(ui);
                    });

                imgui::TreeNode::new(im_str!("scope_general"))
                    .label(im_str!("General"))
                    .default_open(true)
                    .build(ui, || {
                        imgui::InputText::new(ui, im_str!("Name"), &mut imgui::ImString::default()).build();
                        imgui::InputText::new(ui, im_str!("Audio File"), &mut imgui::ImString::default()).build();
                    });

                imgui::TreeNode::new(im_str!("scope_appearance"))
                    .label(im_str!("Appearance"))
                    .build(ui, || {
                        imgui::Drag::new(im_str!("Position")).build_array(ui, &mut [0, 0]);
                        imgui::Drag::new(im_str!("Size")).build_array(ui, &mut [0, 0]);
                    });
            });
    }

    pub fn build(&mut self, ui: &imgui::Ui) {
        self.scope_window(ui);
    }
}
