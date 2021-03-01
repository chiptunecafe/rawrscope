use anyhow::{Context, Result};
use winit::{
    event_loop::EventLoop,
    window::{Window, WindowBuilder},
};

pub struct App {
    // Window structures
    event_loop: EventLoop<()>,
    window: Window,
}

impl App {
    pub fn new(args: &crate::Args) -> Result<Self> {
        // Pretty up the project path for the titlebar
        let path_display = args
            .project_file
            .as_ref()
            .map(|p| format!("{}", p.display()))
            .unwrap_or_else(|| String::from("new project"));

        // Open a window
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_inner_size(winit::dpi::PhysicalSize::new(1600.0, 900.0))
            .with_title(format!("rawrscope ({})", path_display)) // TODO include project path
            .with_resizable(true)
            .build(&event_loop)
            .context("Failed to open a window")?;

        Ok(Self { event_loop, window })
    }

    pub fn run(mut self) -> ! {
        self.event_loop.run(|_, _, _| {})
    }
}
