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
    fft_out: Vec<Complex<f32>>,
    yin: Vec<f32>,
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
}

impl centering::Algorithm for FundamentalPhase {
    fn center(&mut self, data: &[f32], center_range: &RangeInclusive<usize>) -> usize {
        // Slice input buffer to what we want to analyze the pitch of
        let yin_input = &data[*center_range.start()..*center_range.end()];

        // Convenience variables
        let audio_len = yin_input.len();
        let yin_len = yin_input.len() / 2;

        // Measure signal bias
        let bias = yin_input.iter().sum::<f32>() / audio_len as f32;

        // Resize working buffers
        self.buffers.fft_out.resize(audio_len, Zero::zero());
        self.buffers.yin.resize(yin_len, 0.0);

        // Fill FFT input buffer
        self.buffers.fft_in = yin_input
            .iter()
            .map(|v| Complex::from(*v - bias)) // Removes bias from signal
            .collect();

        // Perform first autocorrelation FFT
        let ac_fft1 = self.planners.forward.plan_fft(audio_len);
        ac_fft1.process(&mut self.buffers.fft_in, &mut self.buffers.fft_out);

        // Assemble next input using complex conjugates (simplified)
        for i in 0..audio_len {
            let out = self.buffers.fft_out[i];
            self.buffers.fft_in[i] = Complex::from(out.norm_sqr());
        }

        // Perform second autocorrelation FFT
        let ac_fft2 = self.planners.inverse.plan_fft(audio_len);
        ac_fft2.process(&mut self.buffers.fft_in, &mut self.buffers.fft_out);

        // Convert ACF to YIN SDF
        // Not sure why this works... copied from another implementation.
        for i in 0..yin_len {
            self.buffers.yin[i] = (self.buffers.fft_out[0] + self.buffers.fft_out[1]
                - 2.0 * self.buffers.fft_out[i])
                .re;
        }

        // Compute cumulative mean normalized difference
        self.buffers.yin[0] = 1.0;
        let mut running_sum = 0.0;
        for i in 1..yin_len {
            running_sum += self.buffers.yin[i];
            self.buffers.yin[i] *= i as f32 / running_sum;
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

        // TODO Maybe implement the rest of YIN

        // Assemble single cycle of the signal to perform FFT over
        let mut cycle_data = yin_input[0..tau]
            .iter()
            .map(Complex::from)
            .collect::<Vec<_>>();
        let mut cycle_out = vec![Zero::zero(); tau];

        // Perform single-cycle FFT
        let cycle_fft = self.planners.forward.plan_fft(tau);
        cycle_fft.process(&mut cycle_data, &mut cycle_out);

        // Extract fundamental phase from FFT
        let fundamental_phase = cycle_out[1].to_polar().1;

        // Compute final center location
        // Adds pi to phase to keep it in range
        center_range.start() + tau - ((fundamental_phase + PI) / (2.0 * PI) * tau as f32) as usize
    }

    fn ui(&mut self, ui: &imgui::Ui) -> bool {
        imgui::Slider::new(&imgui::im_str!("Threshold"), 0.0..=1.0).build(ui, &mut self.threshold)
        /*
        ui.checkbox(
            &imgui::im_str!("Snap to nearest zero crossing"),
            &mut self.snap_to_crossings,
        );
        */
    }
}
