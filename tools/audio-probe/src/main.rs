//! Disposable Phase 0 probe for macOS virtual audio inputs.
//!
//! This intentionally does not depend on ED Compass. It establishes whether
//! Core Audio, through CPAL, exposes a virtual device and delivers continuous,
//! correctly interpreted PCM before the production application is changed.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, FromSample, Sample, SampleFormat, SizedSample, Stream, SupportedStreamConfig};

#[derive(Debug, Parser)]
#[command(about = "Probe a macOS virtual audio input before porting ED Compass")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List input-capable devices and their supported configurations.
    List,
    /// Capture an explicitly selected input and report signal/continuity.
    Capture(CaptureArgs),
}

#[derive(Debug, clap::Args)]
struct CaptureArgs {
    /// Exact ID printed by `list`, or an unambiguous case-insensitive name fragment.
    #[arg(long)]
    device: String,

    /// Stop after this many seconds; zero runs until Ctrl-C.
    #[arg(long, default_value_t = 0)]
    duration: u64,

    /// Write captured samples as 32-bit float WAV at this path.
    #[arg(long)]
    wav: Option<PathBuf>,

    /// Seconds between status lines.
    #[arg(long, default_value_t = 1.0, value_parser = positive_f64)]
    report_interval: f64,
}

#[derive(Debug)]
struct Packet {
    at: Instant,
    frames: usize,
    samples: usize,
    sum_squares: f64,
    peak: f32,
    audio: Option<Vec<f32>>,
}

#[derive(Default)]
struct Totals {
    callbacks: u64,
    frames: u64,
    samples: u64,
    sum_squares: f64,
    peak: f32,
    callback_gaps: u64,
    largest_gap: Duration,
    prior_callback: Option<(Instant, usize)>,
}

impl Totals {
    fn observe(&mut self, packet: &Packet, sample_rate: u32) {
        if let Some((prior_at, prior_frames)) = self.prior_callback {
            let elapsed = packet.at.saturating_duration_since(prior_at);
            let expected = Duration::from_secs_f64(prior_frames as f64 / sample_rate as f64);
            // Scheduling jitter is normal. Flag only a delay that exceeds both
            // twice the preceding buffer duration and ten milliseconds extra.
            let threshold = expected.saturating_mul(2) + Duration::from_millis(10);
            if elapsed > threshold {
                self.callback_gaps += 1;
                self.largest_gap = self.largest_gap.max(elapsed);
            }
        }
        self.prior_callback = Some((packet.at, packet.frames));
        self.callbacks += 1;
        self.frames += packet.frames as u64;
        self.samples += packet.samples as u64;
        self.sum_squares += packet.sum_squares;
        self.peak = self.peak.max(packet.peak);
    }

    fn rms(&self) -> f64 {
        if self.samples == 0 {
            0.0
        } else {
            (self.sum_squares / self.samples as f64).sqrt()
        }
    }
}

fn positive_f64(value: &str) -> std::result::Result<f64, String> {
    let parsed: f64 = value.parse().map_err(|_| "expected a number".to_owned())?;
    if parsed.is_finite() && parsed > 0.0 {
        Ok(parsed)
    } else {
        Err("expected a positive finite number".to_owned())
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::List => list_devices(),
        Command::Capture(args) => capture(args),
    }
}

fn list_devices() -> Result<()> {
    let host = cpal::default_host();
    println!("Host: {}", host.id().name());
    println!("Input-capable devices:");

    let default_id = host
        .default_input_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.to_string());
    let mut count = 0usize;

    for device in host.input_devices().context("enumerating input devices")? {
        count += 1;
        let id = device
            .id()
            .map(|id| id.to_string())
            .unwrap_or_else(|error| format!("<unavailable: {error}>"));
        let description = device
            .description()
            .map(|value| value.to_string())
            .unwrap_or_else(|error| format!("<unavailable: {error}>"));
        let marker = if default_id.as_deref() == Some(id.as_str()) {
            " [default input]"
        } else {
            ""
        };
        println!("\n{count}. {description}{marker}\n   ID: {id}");

        match device.default_input_config() {
            Ok(config) => println!("   Default: {}", describe_config(&config)),
            Err(error) => println!("   Default: unavailable ({error})"),
        }
        match device.supported_input_configs() {
            Ok(configs) => {
                for config in configs {
                    println!(
                        "   Supports: {} ch, {}–{} Hz, {}",
                        config.channels(),
                        config.min_sample_rate(),
                        config.max_sample_rate(),
                        config.sample_format()
                    );
                }
            }
            Err(error) => println!("   Supported formats: unavailable ({error})"),
        }
    }

    if count == 0 {
        println!("  (none)");
    }
    Ok(())
}

fn capture(args: CaptureArgs) -> Result<()> {
    let host = cpal::default_host();
    let device = select_device(&host, &args.device)?;
    let id = device.id().context("reading selected device ID")?;
    let description = device
        .description()
        .context("reading selected device description")?;
    let supported = device
        .default_input_config()
        .context("selected device has no default input configuration")?;
    let config: cpal::StreamConfig = supported.into();

    println!("Selected: {description}");
    println!("ID: {id}");
    println!("Format: {}", describe_config(&supported));
    println!("Buffer size: {:?}", config.buffer_size);
    if let Some(path) = &args.wav {
        println!("WAV: {}", path.display());
    }
    println!("Starting capture; press Ctrl-C to stop.");

    let running = Arc::new(AtomicBool::new(true));
    let signal_flag = Arc::clone(&running);
    ctrlc::set_handler(move || signal_flag.store(false, Ordering::SeqCst))
        .context("installing Ctrl-C handler")?;

    let dropped_packets = Arc::new(AtomicU64::new(0));
    let stream_errors = Arc::new(AtomicU64::new(0));
    let (packet_tx, packet_rx) = std::sync::mpsc::sync_channel::<Packet>(1024);
    let (error_tx, error_rx) = std::sync::mpsc::channel::<String>();
    let include_audio = args.wav.is_some();
    let channels = config.channels as usize;

    let stream = build_stream(
        &device,
        &config,
        supported.sample_format(),
        packet_tx,
        error_tx,
        Arc::clone(&dropped_packets),
        Arc::clone(&stream_errors),
        channels,
        include_audio,
    )?;

    let mut writer = args
        .wav
        .as_ref()
        .map(|path| {
            hound::WavWriter::create(
                path,
                hound::WavSpec {
                    channels: config.channels,
                    sample_rate: config.sample_rate,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                },
            )
            .with_context(|| format!("creating {}", path.display()))
        })
        .transpose()?;

    stream.play().context("starting input stream")?;
    let started = Instant::now();
    let deadline = (args.duration > 0).then(|| started + Duration::from_secs(args.duration));
    let report_every = Duration::from_secs_f64(args.report_interval);
    let mut next_report = started + report_every;
    let mut interval = Totals::default();
    let mut total = Totals::default();
    let mut terminal_error: Option<String> = None;

    while running.load(Ordering::SeqCst) && deadline.is_none_or(|end| Instant::now() < end) {
        while let Ok(error) = error_rx.try_recv() {
            eprintln!("Stream error: {error}");
            terminal_error = Some(error);
            running.store(false, Ordering::SeqCst);
        }

        match packet_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(packet) => {
                interval.observe(&packet, config.sample_rate);
                total.observe(&packet, config.sample_rate);
                if let (Some(writer), Some(audio)) = (writer.as_mut(), packet.audio) {
                    for sample in audio {
                        writer.write_sample(sample).context("writing WAV samples")?;
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                terminal_error = Some("audio callback channel disconnected".to_owned());
                break;
            }
        }

        if Instant::now() >= next_report {
            println!(
                "{:>7.1}s  callbacks {:>6}  frames {:>9}  RMS {:>7.4} ({:>6.1} dBFS)  peak {:>7.4}  gaps {:>3}  dropped {:>3}  errors {}",
                started.elapsed().as_secs_f64(),
                interval.callbacks,
                interval.frames,
                interval.rms(),
                amplitude_dbfs(interval.rms()),
                interval.peak,
                interval.callback_gaps,
                dropped_packets.load(Ordering::Relaxed),
                stream_errors.load(Ordering::Relaxed),
            );
            interval = Totals::default();
            next_report += report_every;
            if next_report <= Instant::now() {
                next_report = Instant::now() + report_every;
            }
        }
    }

    drop(stream);
    while let Ok(packet) = packet_rx.try_recv() {
        total.observe(&packet, config.sample_rate);
        if let (Some(writer), Some(audio)) = (writer.as_mut(), packet.audio) {
            for sample in audio {
                writer
                    .write_sample(sample)
                    .context("writing final WAV samples")?;
            }
        }
    }
    if let Some(writer) = writer {
        writer.finalize().context("finalizing WAV")?;
    }

    let elapsed = started.elapsed();
    let audio_seconds = total.frames as f64 / config.sample_rate as f64;
    println!("\nCapture summary");
    println!("  Wall time: {:.3} s", elapsed.as_secs_f64());
    println!("  Audio frames: {} ({audio_seconds:.3} s)", total.frames);
    println!("  Callbacks: {}", total.callbacks);
    println!(
        "  Overall RMS: {:.6} ({:.1} dBFS)",
        total.rms(),
        amplitude_dbfs(total.rms())
    );
    println!("  Peak: {:.6}", total.peak);
    println!("  Suspected callback gaps: {}", total.callback_gaps);
    println!(
        "  Largest callback interval: {:.3} ms",
        total.largest_gap.as_secs_f64() * 1000.0
    );
    println!(
        "  Probe-channel drops: {}",
        dropped_packets.load(Ordering::Relaxed)
    );
    println!("  Stream errors: {}", stream_errors.load(Ordering::Relaxed));

    if let Some(error) = terminal_error {
        bail!("capture ended after a stream failure: {error}");
    }
    Ok(())
}

fn select_device(host: &cpal::Host, selector: &str) -> Result<Device> {
    if let Ok(id) = selector.parse()
        && let Some(device) = host.device_by_id(&id)
    {
        if device.supports_input() {
            return Ok(device);
        }
        bail!("device {selector} exists but does not support input");
    }

    let needle = selector.to_lowercase();
    let mut matches = Vec::new();
    for device in host.input_devices().context("enumerating input devices")? {
        let description = device
            .description()
            .map(|value| value.to_string())
            .unwrap_or_default();
        if description.to_lowercase().contains(&needle) {
            matches.push((description, device));
        }
    }
    match matches.len() {
        0 => bail!("no input device matches {selector:?}; run `list` to inspect devices"),
        1 => Ok(matches.pop().expect("one match").1),
        _ => {
            let names = matches
                .into_iter()
                .map(|(name, _)| format!("  - {name}"))
                .collect::<Vec<_>>()
                .join("\n");
            bail!(
                "device selector {selector:?} is ambiguous:\n{names}\nuse the exact ID from `list`"
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_stream(
    device: &Device,
    config: &cpal::StreamConfig,
    format: SampleFormat,
    packet_tx: SyncSender<Packet>,
    error_tx: std::sync::mpsc::Sender<String>,
    dropped_packets: Arc<AtomicU64>,
    stream_errors: Arc<AtomicU64>,
    channels: usize,
    include_audio: bool,
) -> Result<Stream> {
    macro_rules! stream_for {
        ($sample:ty) => {{
            let dropped = Arc::clone(&dropped_packets);
            let errors = Arc::clone(&stream_errors);
            let errors_tx = error_tx.clone();
            device.build_input_stream(
                config.clone(),
                move |data: &[$sample], _| {
                    send_packet(data, channels, include_audio, &packet_tx, &dropped)
                },
                move |error| {
                    errors.fetch_add(1, Ordering::Relaxed);
                    let _ = errors_tx.send(error.to_string());
                },
                None,
            )
        }};
    }

    let stream = match format {
        SampleFormat::I8 => stream_for!(i8),
        SampleFormat::I16 => stream_for!(i16),
        SampleFormat::I32 => stream_for!(i32),
        SampleFormat::I64 => stream_for!(i64),
        SampleFormat::U8 => stream_for!(u8),
        SampleFormat::U16 => stream_for!(u16),
        SampleFormat::U32 => stream_for!(u32),
        SampleFormat::U64 => stream_for!(u64),
        SampleFormat::F32 => stream_for!(f32),
        SampleFormat::F64 => stream_for!(f64),
        other => bail!("sample format {other} is not supported by this probe"),
    }
    .context("building input stream")?;
    Ok(stream)
}

fn send_packet<T>(
    data: &[T],
    channels: usize,
    include_audio: bool,
    tx: &SyncSender<Packet>,
    dropped: &AtomicU64,
) where
    T: Sample + SizedSample + Copy,
    f32: FromSample<T>,
{
    let mut peak = 0.0f32;
    let mut sum_squares = 0.0f64;
    let mut audio = include_audio.then(|| Vec::with_capacity(data.len()));
    for &input in data {
        let sample = f32::from_sample(input);
        peak = peak.max(sample.abs());
        sum_squares += f64::from(sample) * f64::from(sample);
        if let Some(audio) = &mut audio {
            audio.push(sample);
        }
    }
    let packet = Packet {
        at: Instant::now(),
        frames: data.len() / channels.max(1),
        samples: data.len(),
        sum_squares,
        peak,
        audio,
    };
    if let Err(error) = tx.try_send(packet)
        && matches!(error, TrySendError::Full(_))
    {
        dropped.fetch_add(1, Ordering::Relaxed);
    }
}

fn describe_config(config: &SupportedStreamConfig) -> String {
    format!(
        "{} Hz, {} channel(s), {}, {:?}",
        config.sample_rate(),
        config.channels(),
        config.sample_format(),
        config.buffer_size()
    )
}

fn amplitude_dbfs(amplitude: f64) -> f64 {
    if amplitude > 0.0 {
        20.0 * amplitude.log10()
    } else {
        f64::NEG_INFINITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_interval_must_be_positive_and_finite() {
        assert!(positive_f64("1").is_ok());
        assert!(positive_f64("0").is_err());
        assert!(positive_f64("-1").is_err());
        assert!(positive_f64("NaN").is_err());
    }

    #[test]
    fn dbfs_has_expected_reference_points() {
        assert_eq!(amplitude_dbfs(1.0), 0.0);
        assert!((amplitude_dbfs(0.5) + 6.0206).abs() < 0.001);
        assert!(amplitude_dbfs(0.0).is_infinite());
    }
}
