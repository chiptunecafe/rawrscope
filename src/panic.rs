use std::panic;

use tinyfiledialogs as tfd;

type PanicHandler = Box<dyn Fn(&panic::PanicInfo) + Sync + Send + 'static>;

pub fn dialog(old_handler: PanicHandler) -> PanicHandler {
    Box::new(move |info: &panic::PanicInfo| {
        old_handler(info);

        // should always exist according to default panic hook comments
        let location = info.location().unwrap();

        let info = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "No information available...",
            },
        };

        tfd::message_box_ok(
            "Oops!",
            &format!(
                "rawrscope encontered an unrecoverable error!\n{}\n(at {})",
                info, location
            ),
            tfd::MessageBoxIcon::Error,
        );
    })
}
