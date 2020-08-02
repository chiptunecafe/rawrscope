use clap::{clap_app, crate_version, AppSettings};

pub fn get() -> clap::App<'static, 'static> {
    clap_app!(rawrscope =>
        (global_setting: AppSettings::DisableHelpSubcommand)
        (global_setting: AppSettings::VersionlessSubcommands)

        (version: crate_version!())
        (author: "Max Beck <rytonemail@gmail.com>")

        (@arg PROJECT: "Project file to open")

        (@subcommand configure_audio =>
            (about: "Select audio host and output")
        )
    )
}
