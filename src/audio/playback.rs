use std::sync::Arc;
use std::thread;

use cpal::{
    traits::{DeviceTrait, EventLoopTrait, HostTrait},
    UnknownTypeOutputBuffer as UOut,
};
use parking_lot::Mutex;
use sample::Sample;
use snafu::{OptionExt, ResultExt, Snafu};

use crate::audio::mixer;

#[derive(Debug, Snafu)]
pub enum CreateError {
    #[snafu(display("No output device available on host \"{:?}\"", host))]
    NoOutputDevice { host: cpal::HostId },

    #[snafu(display("Audio device initialization panicked!"))]
    InitializationPanic,

    #[snafu(display("Failed to get output format for device: {}", source))]
    NoOutputFormats { source: cpal::DefaultFormatError },

    #[snafu(display("Could not create mixer: {}", source))]
    MixerError { source: samplerate::Error },

    #[snafu(display("Failed to initialize audio output stream: {}", source))]
    StreamCreateError { source: cpal::BuildStreamError },

    #[snafu(display("Failed to start audio output stream: {}", source))]
    StreamPlayError { source: cpal::PlayStreamError },

    #[snafu(display("Failed to start audio thread: {}", source))]
    ThreadError { source: std::io::Error },
}

fn audio_host(config: &crate::config::Audio) -> cpal::Host {
    match &config.host {
        Some(host_name) => {
            if let Some((id, _n)) = cpal::available_hosts()
                .iter()
                .map(|host_id| (host_id, format!("{:?}", host_id)))
                .find(|(_id, n)| n == host_name)
            {
                cpal::host_from_id(*id).unwrap_or_else(|err| {
                    log::warn!(
                        "Could not use host \"{}\": {}, using default...",
                        host_name,
                        err
                    );
                    cpal::default_host()
                })
            } else {
                log::warn!("Host \"{}\" does not exist! Using default...", host_name);
                cpal::default_host()
            }
        }
        None => cpal::default_host(),
    }
}

fn audio_device(
    config: &crate::config::Audio,
    host: &cpal::Host,
) -> Result<cpal::Device, CreateError> {
    match &config.device {
        Some(dev_name) => match host.output_devices() {
            Ok(mut iter) => match iter.find(|dev| {
                dev.name()
                    .ok()
                    .map(|name| &name == dev_name)
                    .unwrap_or(false)
            }) {
                Some(d) => Ok(d),
                None => {
                    log::warn!(
                        "Output device \"{}\" does not exist ... using default",
                        dev_name
                    );
                    host.default_output_device()
                        .context(NoOutputDevice { host: host.id() })
                }
            },
            Err(e) => {
                log::warn!(
                    "Failed to query output devices: {} ... attempting to use default",
                    e
                );
                host.default_output_device()
                    .context(NoOutputDevice { host: host.id() })
            }
        },
        None => host
            .default_output_device()
            .context(NoOutputDevice { host: host.id() }),
    }
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
    pub fn new(config: &crate::config::Config) -> Result<Self, CreateError> {
        let config = config.audio.clone();
        let (host, device, format) = thread::Builder::new()
            .name("audio init".into())
            .spawn(move || {
                let host = audio_host(&config);
                let device = audio_device(&config, &host)?;
                let format = device.default_output_format().context(NoOutputFormats)?;
                Ok((host, device, format))
            })
            .context(ThreadError)?
            .join()
            .ok()
            .context(InitializationPanic)??;

        let (submission_queue, sub_rx) = crossbeam_channel::bounded(0);
        let mut mixer_builder = mixer::MixerBuilder::new();
        mixer_builder.channels(format.channels as usize);
        mixer_builder.target_sample_rate(format.sample_rate.0);
        let mixer = mixer_builder
            .build(sub_rx.into_iter())
            .context(MixerError)?;
        let submission_builder = mixer.submission_builder();
        let mixer_stream = Arc::new(Mutex::new(mixer.into_stream()));

        log::debug!("Starting audio thread: format={:?}", format);
        let audio_stream = mixer_stream.clone();
        let thr_format = format.clone();
        let audio_thread = thread::Builder::new()
            .name("audio playback".into())
            .spawn(move || {
                let ev = host.event_loop();
                let res: Result<(), CreateError> = (move || {
                    let stream_id = ev
                        .build_output_stream(&device, &thr_format)
                        .context(StreamCreateError)?;

                    ev.play_stream(stream_id).context(StreamPlayError)?;

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
                    });
                })();

                if let Err(e) = res {
                    log::error!("Unexpected audio thread error! {}", e);
                }
            })
            .context(ThreadError)?;

        Ok(Self {
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
