use std::thread;

use cpal::{
    traits::{DeviceTrait, EventLoopTrait, HostTrait},
    UnknownTypeOutputBuffer as UOut,
};
use failure::Fail;
use sample::{Sample, Signal};
use snafu::{OptionExt, ResultExt, Snafu};

use crate::audio::mixer;

// cpal only impls failure::Fail on its errors
// meaning that i have to add failure as a dependency
// just for compat with std::error::Error
// rree

#[derive(Debug, Snafu)]
pub enum CreateError {
    #[snafu(display("Failed to query supported output formats: {}", source))]
    FormatQueryError {
        source: failure::Compat<cpal::SupportedFormatsError>,
    },
    #[snafu(display("No available audio output formats for the selected device!"))]
    NoOutputFormats,
    #[snafu(display("Failed to initialize audio output stream: {}", source))]
    StreamCreateError {
        source: failure::Compat<cpal::BuildStreamError>,
    },
    #[snafu(display("Failed to start audio output stream: {}", source))]
    StreamPlayError {
        source: failure::Compat<cpal::PlayStreamError>,
    },
}

pub struct Player {
    audio_thread: thread::JoinHandle<()>,
    submission_queues: Vec<crossbeam_channel::Sender<mixer::Submission>>,
    channels: u16,
}

impl Player {
    pub fn new(host: cpal::Host, device: cpal::Device) -> Result<Self, CreateError> {
        let ev = host.event_loop();

        let formats = device
            .supported_output_formats()
            .map_err(|e| e.compat()) // not using failure::ResultExt due to conflict with snafu
            .context(FormatQueryError)?;

        let format = formats
            .max_by(cpal::SupportedFormat::cmp_default_heuristics)
            .context(NoOutputFormats)?
            .with_max_sample_rate();

        let mut submission_queues = Vec::new();
        let mut mixers = Vec::new();

        for _ in 0..format.channels {
            let (mixer, sub) =
                mixer::Mixer::<mixer::SincResampler>::new(Some(format.sample_rate.0));

            submission_queues.push(sub);
            mixers.push(mixer);
        }

        let stream_id = ev
            .build_output_stream(&device, &format)
            .map_err(|e| e.compat())
            .context(StreamCreateError)?;

        ev.play_stream(stream_id)
            .map_err(|e| e.compat())
            .context(StreamPlayError)?;

        log::debug!("Starting audio thread: format={:?}", format);
        let audio_thread = thread::spawn(move || {
            ev.run(move |_stream_id, stream_res| {
                let stream_data = match stream_res {
                    Ok(data) => data,
                    Err(err) => {
                        log::error!("Audio playback stream error: {}", err);
                        return;
                    }
                };

                match stream_data {
                    cpal::StreamData::Output {
                        buffer: UOut::U16(mut buffer),
                    } => {
                        for (i, elem) in buffer.iter_mut().enumerate() {
                            // TODO use channels instead of mixers len?
                            let channel = i % mixers.len();
                            let sample = mixers[channel].next();
                            *elem = sample[0].to_sample();
                        }
                    }
                    cpal::StreamData::Output {
                        buffer: UOut::I16(mut buffer),
                    } => {
                        for (i, elem) in buffer.iter_mut().enumerate() {
                            // TODO use channels instead of mixers len?
                            let channel = i % mixers.len();
                            let sample = mixers[channel].next();
                            *elem = sample[0].to_sample();
                        }
                    }
                    cpal::StreamData::Output {
                        buffer: UOut::F32(mut buffer),
                    } => {
                        for (i, elem) in buffer.iter_mut().enumerate() {
                            // TODO use channels instead of mixers len?
                            let channel = i % mixers.len();
                            let sample = mixers[channel].next();
                            *elem = sample[0];
                        }
                    }
                    _ => (),
                }
            })
        });

        Ok(Player {
            audio_thread,
            submission_queues,
            channels: format.channels,
        })
    }

    pub fn submit(
        &self,
        channel: usize,
        submission: mixer::Submission,
    ) -> Result<(), crossbeam_channel::SendError<mixer::Submission>> {
        match self.submission_queues.get(channel) {
            Some(queue) => queue.send(submission),
            None => {
                log::warn!("Attempted to submit to non-existent channel {}!", channel);
                Ok(())
            }
        }
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }
}
