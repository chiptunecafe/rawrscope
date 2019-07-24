use std::panic;

use tinyfiledialogs as tfd;

type PanicHandler = Box<dyn Fn(&panic::PanicInfo) + Sync + Send + 'static>;

pub fn dialog(old_handler: PanicHandler) -> PanicHandler {
    Box::new(move |info: &panic::PanicInfo| {
        old_handler(info);

        let info = if let Some(info) = info.payload().downcast_ref::<&str>() {
            info
        } else {
            "No additional information."
        };

        tfd::message_box_ok(
            "Oops!",
            &format!("rawrscope encontered an unrecoverable error!\n{}", info),
            tfd::MessageBoxIcon::Error,
        );
    })
}
