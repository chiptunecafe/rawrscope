use std::{sync::Arc, thread};

use parking_lot::Mutex;
use winit::event_loop::EventLoopProxy;

pub struct AsyncSwapchain {
    swapchain: Arc<Mutex<wgpu::SwapChain>>,
    presented: bool,
    thread: thread::JoinHandle<()>,
}

pub type Event = Result<wgpu::SwapChainFrame, wgpu::SwapChainError>;

impl AsyncSwapchain {
    pub fn new(initial_swapchain: wgpu::SwapChain, proxy: EventLoopProxy<Event>) -> Self {
        let swapchain = Arc::new(Mutex::new(initial_swapchain));

        let sc = swapchain.clone();
        let thread = thread::spawn(move || loop {
            // Wait until an image has been requested
            thread::park();

            // Lock down swapchain for duration of loop iteration
            let swapchain = sc.lock();
            tracing::trace!("Woke to acquire image and locked down swapchain");

            // Get next swapchain image
            let image = swapchain.get_current_frame();

            // Exit thread if the event loop has closed
            if let Err(_) = proxy.send_event(image) {
                break;
            }

            tracing::trace!("Acquired and sent off swapchain image");
        });

        Self {
            swapchain,
            presented: true,
            thread,
        }
    }

    pub fn presented(&self) -> bool {
        self.presented
    }

    pub fn replace_swapchain(&mut self, sc: wgpu::SwapChain) {
        *self.swapchain.lock() = sc;
    }

    pub fn request_image(&mut self) {
        if self.presented {
            self.presented = false;

            // Wake up swapchain thread to acquire new image
            self.thread.thread().unpark();
        }
    }

    pub fn notify_presented(&mut self) {
        self.presented = true;
    }
}
