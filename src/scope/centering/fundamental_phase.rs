use std::f32::consts::PI;
use std::ops::RangeInclusive;

use derivative::Derivative;
use rustfft::{num_complex::Complex, num_traits::Zero, FFTplanner};
use serde::{Deserialize, Serialize};

use crate::scope::centering;

struct Planners {
    forward: FFTplanner<f32>,
    inverse: FFTplanner<f32>,
}

impl Default for Planners {
    fn default() -> Self {
        Self {
            forward: FFTplanner::new(false),
            inverse: FFTplanner::new(true),
        }
    }
}

#[derive(Default)]
struct Buffers {
    fft_in: Vec<Complex<f32>>,
    kernel_out: Vec<Complex<f32>>,
    fft_out: Vec<Complex<f32>>,
    yin: Vec<f32>,
    power_terms: Vec<f32>,
}

#[derive(Deserialize, Serialize, Derivative)]
#[derivative(Default)]
pub struct FundamentalPhase {
    #[derivative(Default(value = "0.5"))]
    threshold: f32,
    snap_to_crossings: bool,

    #[serde(skip)]
    planners: Planners,
    #[serde(skip)]
    buffers: Buffers,
    #[serde(skip)]
    last_tau: usize,
}

impl centering::Algorithm for FundamentalPhase {
    fn center(&mut self, data: &[f32], center_range: &RangeInclusive<usize>) -> usize {
        // Most of the YIN implementation is ported from here:
        // https://github.com/JorenSix/TarsosDSP
        // Some improvements were made, particularly with power term calculation,
        // in order to improve stabilitiy.
        //
        // "All-phase FFT" is described in the paper "New method of estimation
        // of phase, amplitude, and frequency based on all phase FFT spectrum
        // analysis" from Huang Xiaohong, Wang Zhaohua, and Hou Guoqiang.
        // It's currently only implemented for future experimentation... right
        // now it is pointless, and using just Goertzel's algorithm would be
        // much more efficient.

        // Slice input buffer to what we want to analyze the pitch of
        let yin_input = &data[*center_range.start()..*center_range.end()];

        // Convenience variables
        let audio_len = yin_input.len();
        let yin_len = yin_input.len() / 2;

        // Resize working buffers
        self.buffers.kernel_out.resize(audio_len, Zero::zero());
        self.buffers.fft_out.resize(audio_len, Zero::zero());
        self.buffers.yin.resize(yin_len, 0.0);
        self.buffers.power_terms.resize(yin_len, 0.0);

        // Fill FFT input buffer
        self.buffers.fft_in = yin_input.iter().map(Complex::from).collect();

        // Perform first autocorrelation FFT
        let ac_fft1 = self.planners.forward.plan_fft(audio_len);
        ac_fft1.process(&mut self.buffers.fft_in, &mut self.buffers.fft_out);

        // Create convolution kernel
        for i in 0..yin_len {
            self.buffers.fft_in[i] = Complex::from(yin_input[yin_len - i]);
        }
        for i in yin_len..audio_len {
            self.buffers.fft_in[i] = Zero::zero();
        }
        ac_fft1.process(&mut self.buffers.fft_in, &mut self.buffers.kernel_out);

        // Apply convolution kernel
        for i in 0..audio_len {
            let out = self.buffers.fft_out[i];
            let kern = self.buffers.kernel_out[i];
            self.buffers.fft_in[i] = out * kern / (audio_len as f32).sqrt();
        }

        // Perform second autocorrelation FFT
        let ac_fft2 = self.planners.inverse.plan_fft(audio_len);
        ac_fft2.process(&mut self.buffers.fft_in, &mut self.buffers.fft_out);

        // Iteratively estimate power terms from first autocorrelation output
        self.buffers.power_terms[0] = self.buffers.fft_out[yin_len].re / (audio_len as f32).sqrt();
        for tau in 1..yin_len {
            let last_v = yin_input[tau - 1];
            let next_v = yin_input[yin_len + tau - 1];

            self.buffers.power_terms[tau] =
                self.buffers.power_terms[tau - 1] - last_v * last_v + next_v * next_v;
        }

        // Convert ACF to YIN SDF
        for i in 0..yin_len {
            self.buffers.yin[i] = self.buffers.power_terms[0] + self.buffers.power_terms[i]
                - 2.0 * self.buffers.fft_out[i + yin_len].re / (audio_len as f32).sqrt();
        }

        // Compute cumulative mean normalized difference
        self.buffers.yin[0] = 1.0;
        let mut running_sum = 0.0;
        for tau in 1..yin_len {
            running_sum += self.buffers.yin[tau].max(0.0); // clamped to account for error caused by fft
            self.buffers.yin[tau] *= tau as f32 / running_sum;
        }

        // Pick final tau value
        let mut tau = 2;
        while tau < yin_len {
            if self.buffers.yin[tau] < self.threshold {
                while tau + 1 < yin_len && self.buffers.yin[tau + 1] < self.buffers.yin[tau] {
                    tau += 1;
                }
                break;
            }
            tau += 1;
        }

        self.last_tau = tau;

        // TODO Implement the rest of YIN

        // Assemble two cycles of the signal to perform apFFT over
        tau *= 2; // dirty way to get two cycles

        let cycle_data = yin_input[0..tau]
            .iter()
            .enumerate()
            .map(|(i, v)| {
                // Triangular window
                let l = tau as i32 / 2;
                let window = (l - (i as i32 - l).abs()) as f32 / l as f32;
                Complex::from(v * window)
            })
            .collect::<Vec<_>>();

        let mut cycle_data_folded = Vec::with_capacity(tau);
        for v in &cycle_data[tau / 2..tau] {
            cycle_data_folded.push(*v);
        }
        for v in &cycle_data[0..tau / 2] {
            cycle_data_folded.push(*v);
        }

        let mut cycle_out = vec![Zero::zero(); tau];

        // Perform FFT
        let cycle_fft = self.planners.forward.plan_fft(tau);
        cycle_fft.process(&mut cycle_data_folded, &mut cycle_out);

        // Extract fundamental phase from FFT
        let fundamental_phase = cycle_out[2].im.atan2(cycle_out[2].re);

        // TODO Experiment with ideas to remove phase shifting (i.e. FM waves)

        tau /= 2;

        // Compute final center location
        // Adds pi to phase to keep it in range
        let center = *center_range.start() + tau
            - ((fundamental_phase + PI) / (2.0 * PI) * tau as f32) as usize;

        // Snap to next zero crossing (if enabled)
        if self.snap_to_crossings {
            for i in center..center + tau {
                if data[i - 1].is_sign_negative() && data[i].is_sign_positive() {
                    return i;
                }
            }
        }

        center
    }

    fn ui(&mut self, ui: &imgui::Ui) -> bool {
        ui.text(format!("tau={}", self.last_tau));
        imgui::Slider::new(&imgui::im_str!("Threshold"), 0.0..=1.0).build(ui, &mut self.threshold)
            | ui.checkbox(
                &imgui::im_str!("Snap to next zero crossing within cycle"),
                &mut self.snap_to_crossings,
            )
    }
}
