use std::io::{BufRead, Write};

use cpal::traits::{DeviceTrait, HostTrait};

use crate::config;

// TODO maybe use snafu to clean up error handling
pub fn run() {
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock().lines();

    println!("Select audio host:");

    let hosts = cpal::available_hosts();
    for (i, host_id) in hosts.iter().enumerate() {
        println!("{:>3}: {:?}", i, host_id);
    }

    let host_id = loop {
        print!("> ");
        if let Err(e) = std::io::stdout().flush() {
            tracing::error!("{}", e);
            return;
        }

        let idx_str = match stdin.next() {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                tracing::error!("{}", e);
                return;
            }
            None => {
                tracing::error!("No lines available from stdin!");
                return;
            }
        };

        if let Ok(idx) = idx_str.parse::<usize>() {
            if let Some(host) = hosts.get(idx) {
                break host;
            }
        }
    };

    let host = match cpal::host_from_id(*host_id) {
        Ok(host) => host,
        Err(e) => {
            tracing::error!("{}", e);
            return;
        }
    };

    println!("Select audio output device:");

    let device_iter = match host.output_devices() {
        Ok(iter) => iter,
        Err(e) => {
            tracing::error!("{}", e);
            return;
        }
    };
    let devices = device_iter.collect::<Vec<_>>();
    if devices.is_empty() {
        tracing::error!("No available output devices!");
        return;
    }

    for (i, device) in devices.iter().enumerate() {
        match device.name() {
            Ok(name) => println!("{:>3}: {}", i, name),
            Err(e) => tracing::warn!("Failed to get name of device {}: {}", i, e),
        }
    }

    let device = loop {
        print!("> ");
        if let Err(e) = std::io::stdout().flush() {
            tracing::error!("{}", e);
            return;
        }

        let idx_str = match stdin.next() {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                tracing::error!("{}", e);
                return;
            }
            None => {
                tracing::error!("No lines available from stdin!");
                return;
            }
        };

        if let Ok(idx) = idx_str.parse::<usize>() {
            if let Some(device) = devices.get(idx) {
                break device;
            }
        }
    };

    let mut config = config::Config::load();
    config.audio.host = Some(format!("{:?}", host_id));
    config.audio.device = device.name().ok();

    if let Err(e) = config.write() {
        tracing::error!("{}", e);
    }
}
