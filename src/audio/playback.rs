use std::sync::Arc;
use std::thread;

use cpal::{
    traits::{DeviceTrait, EventLoopTrait, HostTrait},
    UnknownTypeOutputBuffer as UOut,
};
use failure::Fail;
use parking_lot::Mutex;
use sample::Sample;
use snafu::{ResultExt, Snafu};

use crate::audio::mixer;

// cpal only impls failure::Fail on its errors
// meaning that i have to add failure as a dependency
// just for compat with std::error::Error
// rree

#[derive(Debug, Snafu)]
pub enum CreateError {
    #[snafu(display("Failed to get output format for device: {}", source))]
    NoOutputFormats {
        source: failure::Compat<cpal::DefaultFormatError>,
    },
    #[snafu(display("Could not create mixer: {}", source))]
    MixerError { source: samplerate::Error },
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
    submission_builder: mixer::SubmissionBuilder,
    submission_queue: crossbeam_channel::Sender<mixer::Submission>,
    mixer_stream: Arc<Mutex<mixer::MixerStream<crossbeam_channel::IntoIter<mixer::Submission>>>>,
    channels: u16,
    sample_rate: u32,
}

impl Player {
    pub fn new(host: cpal::Host, device: cpal::Device) -> Result<Self, CreateError> {
        let ev = host.event_loop();

        let format = device
            .default_output_format()
            .map_err(|e| e.compat())
            .context(NoOutputFormats)?;

        let (submission_queue, sub_rx) = crossbeam_channel::unbounded();
        let mut mixer_builder = mixer::MixerBuilder::new();
        mixer_builder.channels(format.channels as usize);
        mixer_builder.target_sample_rate(format.sample_rate.0);
        let mixer = mixer_builder
            .build(sub_rx.into_iter())
            .context(MixerError)?;
        let submission_builder = mixer.submission_builder();
        let mixer_stream = Arc::new(Mutex::new(mixer.into_stream()));

        let stream_id = ev
            .build_output_stream(&device, &format)
            .map_err(|e| e.compat())
            .context(StreamCreateError)?;

        ev.play_stream(stream_id)
            .map_err(|e| e.compat())
            .context(StreamPlayError)?;

        log::debug!("Starting audio thread: format={:?}", format);
        let audio_stream = mixer_stream.clone();
        let audio_thread = thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));

            ev.run(move |_stream_id, stream_res| {
                let stream_data = match stream_res {
                    Ok(data) => data,
                    Err(err) => {
                        log::error!("Audio playback stream error: {}", err);
                        return;
                    }
                };

                let mut audio_stream = audio_stream.lock();

                match stream_data {
                    cpal::StreamData::Output {
                        buffer: UOut::U16(mut buffer),
                    } => {
                        for elem in buffer.iter_mut() {
                            *elem = audio_stream.next().unwrap_or(0f32).to_sample();
                        }
                    }
                    cpal::StreamData::Output {
                        buffer: UOut::I16(mut buffer),
                    } => {
                        for elem in buffer.iter_mut() {
                            *elem = audio_stream.next().unwrap_or(0f32).to_sample();
                        }
                    }
                    cpal::StreamData::Output {
                        buffer: UOut::F32(mut buffer),
                    } => {
                        for elem in buffer.iter_mut() {
                            *elem = audio_stream.next().unwrap_or(0f32);
                        }
                    }
                    _ => (),
                }
            })
        });

        Ok(Player {
            audio_thread,
            submission_builder,
            submission_queue,
            mixer_stream,
            channels: format.channels,
            sample_rate: format.sample_rate.0,
        })
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn submission_builder(&self) -> &mixer::SubmissionBuilder {
        &self.submission_builder
    }

    pub fn rebuild_mixer(&mut self, builder: mixer::MixerBuilder) -> Result<(), samplerate::Error> {
        log::debug!("Rebuilding master mixer...");

        let (submission_queue, sub_rx) = crossbeam_channel::unbounded();
        let mixer = builder.build(sub_rx.into_iter())?;
        let submission_builder = mixer.submission_builder();
        let mixer_stream = mixer.into_stream();

        self.submission_builder = submission_builder;
        self.submission_queue = submission_queue;
        *self.mixer_stream.lock() = mixer_stream;

        Ok(())
    }

    pub fn submit(
        &self,
        sub: mixer::Submission,
    ) -> Result<(), crossbeam_channel::SendError<mixer::Submission>> {
        self.submission_queue.send(sub)
    }
}
