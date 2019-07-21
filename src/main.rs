mod args;
mod audio_source;
mod commands;
mod state;

fn main() {
    let matches = args::get().get_matches();
    match matches.subcommand_name() {
        None => commands::app::run(matches.value_of("PROJECT")),
        _ => unimplemented!(),
    }
}
