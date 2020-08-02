#![windows_subsystem = "console"]

mod args;
mod audio;
mod commands;
mod config;
mod panic;
mod render;
mod scope;
mod state;
mod ui;

fn main() {
    let matches = args::get().get_matches();

    tracing_log::LogTracer::init().expect("Failed to initialize log -> tracing compat");
    let log_sub = tracing_subscriber::fmt()
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .finish();
    tracing::subscriber::set_global_default(log_sub)
        .expect("Failed to set global tracing subscriber");

    match matches.subcommand_name() {
        None => commands::app::run(matches.value_of("PROJECT")),
        Some("configure_audio") => commands::configure_audio::run(),
        _ => unimplemented!(),
    }
}
