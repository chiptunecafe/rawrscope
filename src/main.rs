mod args;
mod audio_source;
mod commands;
mod state;

fn main() {
    let matches = args::get().get_matches();

    let colors = fern::colors::ColoredLevelConfig::new();
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{:>6} {}",
                colors.color(record.level()),
                message,
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()
        .expect("could not initialize logging"); // TODO dont panic?

    match matches.subcommand_name() {
        None => commands::app::run(matches.value_of("PROJECT")),
        _ => unimplemented!(),
    }
}
