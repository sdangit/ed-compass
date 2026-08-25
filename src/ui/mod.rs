//! The desktop window.
//!
//! Render-only: the UI reads the most recent snapshot and never touches the
//! capture path. Snapshots are taken at `analysis_update_hz`, independently of
//! the frame rate, so redrawing faster costs nothing but pixels.

pub mod compass;
pub mod controls;
pub mod events;
pub mod overlay;
pub mod waterfall;
pub mod zoom;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use eframe::egui;

use crate::app::{App, Status};
use crate::audio::device::{self, AudioDevice};
use crate::game_window::{OverlayAnchor, OverlayPlacement, PlotterGap, overlay_placement};
use crate::pipeline::AnalysisSnapshot;

/// Launch the window. Blocks until it closes.
/// Which renderer to draw with.
///
/// Both are compiled in. `glow` is the default because every crash this tool
/// has had in the field was a wgpu validation error in its multi-viewport
/// texture handling; wgpu is kept so it can be selected without a rebuild, and
/// as a fallback for a machine whose OpenGL driver is too old for glow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Glow,
    Wgpu,
}

impl Backend {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "glow" | "opengl" | "gl" => Some(Self::Glow),
            "wgpu" | "dx12" | "vulkan" => Some(Self::Wgpu),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Glow => "glow",
            Self::Wgpu => "wgpu",
        }
    }

    /// The one to try when this one will not start.
    fn fallback(self) -> Self {
        match self {
            Self::Glow => Self::Wgpu,
            Self::Wgpu => Self::Glow,
        }
    }
}

/// The renderer currently drawing, for the crash log to name.
static ACTIVE_BACKEND: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// What the running process is drawing with, or `"none"` before the window opens.
pub fn active_backend() -> &'static str {
    match ACTIVE_BACKEND.load(std::sync::atomic::Ordering::Relaxed) {
        1 => "glow",
        2 => "wgpu",
        _ => "none",
    }
}

fn native_options(backend: Backend, first_launch: bool) -> eframe::NativeOptions {
    // One window. The in-game overlay is a second viewport that shows and
    // hides itself with the game; there is no other shape to switch to, which
    // means there is no state you have to kill the process to leave.
    let (inner_size, min_inner_size) = if first_launch {
        ([760.0, 430.0], [700.0, 400.0])
    } else {
        ([1180.0, 860.0], [900.0, 620.0])
    };
    let viewport = egui::ViewportBuilder::default()
        .with_title("ED Compass")
        .with_inner_size(inner_size)
        .with_min_inner_size(min_inner_size)
        // Transparency is requested here, on the *root* window, even though
        // this window is opaque. eframe's glow backend chooses one GL config
        // for the whole process from this flag, and a config without an alpha
        // channel cannot host a transparent window — so the overlay, a child
        // viewport, would come out as an opaque black rectangle over the game.
        // `clear_color` below keeps this window itself solid.
        // Only Windows creates the transparent cockpit overlay. Asking the Mac
        // root window for an alpha-capable surface adds complexity for no
        // visible benefit.
        .with_transparent(cfg!(windows));

    eframe::NativeOptions {
        viewport,
        renderer: match backend {
            Backend::Glow => eframe::Renderer::Glow,
            Backend::Wgpu => eframe::Renderer::Wgpu,
        },
        ..Default::default()
    }
}

pub fn run(app: App, preferred: Backend) -> Result<()> {
    // The application is handed over only when a window actually opens. If the
    // renderer cannot start, it is still here to give to the other one.
    let slot = std::rc::Rc::new(std::cell::RefCell::new(Some(app)));

    match run_with(std::rc::Rc::clone(&slot), preferred) {
        Ok(()) => Ok(()),
        Err(first) if slot.borrow().is_some() => {
            // Only a *startup* failure can be retried, and the untouched slot
            // is how we know that is what happened: a driver too old for glow,
            // or no working GL at all. A backend that starts and then panics
            // hours later has already taken the process with it, and no
            // fallback here can help — the crash log names the backend instead,
            // so the next launch can be told to use the other one.
            let other = preferred.fallback();
            log::error!(
                "the {} renderer could not start ({first:#}); trying {}",
                preferred.as_str(),
                other.as_str()
            );
            run_with(slot, other).map_err(|second| {
                anyhow::anyhow!(
                    "neither renderer could open a window.\n  {}: {first:#}\n  {}: {second:#}",
                    preferred.as_str(),
                    other.as_str()
                )
            })
        }
        Err(ran_and_failed) => Err(ran_and_failed),
    }
}

fn run_with(slot: std::rc::Rc<std::cell::RefCell<Option<App>>>, backend: Backend) -> Result<()> {
    ACTIVE_BACKEND.store(
        match backend {
            Backend::Glow => 1,
            Backend::Wgpu => 2,
        },
        std::sync::atomic::Ordering::Relaxed,
    );
    log::info!("opening the window with the {} renderer", backend.as_str());

    /// Keep the event loop running whatever the window manager thinks.
    ///
    /// The overlay is a deferred viewport, and egui closes one as soon as a frame
    /// goes by without it being shown. Ours is shown from the per-frame logic —
    /// which eframe stops calling when the main window is occluded or minimized.
    /// Alt-Tab away for long enough and the overlay is not merely hidden, it is
    /// destroyed, and it does not come back until the program is restarted. That
    /// happened in flight, while taking screenshots.
    ///
    /// A thread that asks for a repaint on a fixed cadence removes the dependency
    /// entirely: frames keep happening whether or not anything is visible, so the
    /// viewport is re-shown every time and cannot lapse. It costs one sleeping
    /// thread.
    fn start_repaint_heartbeat(ctx: egui::Context) {
        std::thread::Builder::new()
            .name("repaint-heartbeat".into())
            .spawn(move || {
                loop {
                    // Slow on purpose. This is a floor, not a cadence: the
                    // interface schedules its own repaints and this only has to
                    // ensure frames never stop entirely. Running it at the frame
                    // rate put a second clock beside the first, and two
                    // unsynchronised timers at the same nominal rate deliver
                    // frames in bursts and gaps — which the overlay showed as
                    // judder while the main window, whose picture advances less
                    // than a pixel per update, rode it out.
                    std::thread::sleep(Duration::from_millis(250));
                    ctx.request_repaint();
                }
            })
            .expect("spawning the repaint heartbeat");
    }

    let first_launch = slot
        .borrow()
        .as_ref()
        .is_some_and(|app| !app.config().setup_complete);
    let options = native_options(backend, first_launch);
    eframe::run_native(
        "ED Compass",
        options,
        Box::new(move |cc| {
            start_repaint_heartbeat(cc.egui_ctx.clone());
            let app = slot
                .borrow_mut()
                .take()
                .ok_or_else(|| "the application was already handed to a renderer".to_owned())?;
            apply_appearance(&cc.egui_ctx, &app.config().appearance);
            Ok(Box::new(CompassUi::new(app)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("could not open the window: {e}"))
}

fn apply_appearance(ctx: &egui::Context, appearance: &str) {
    let preference = match appearance {
        "light" => egui::ThemePreference::Light,
        "dark" => egui::ThemePreference::Dark,
        _ => egui::ThemePreference::System,
    };
    ctx.set_theme(preference);
}

/// Make a string safe for a Windows filename.
fn sanitize_for_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
            _ => '-',
        })
        .collect()
}

/// Export height, corrected so a cropped band does not steepen every slope.
///
/// The published spectrograms span 20 Hz to 22050 Hz; showing less magnifies
/// frequency and tilts every stroke unless the height is scaled to match.
pub fn export_height(cfg: &crate::config::Config) -> usize {
    if cfg.export_match_published_aspect {
        cfg.matched_export_height(20.0, 22_050.0)
    } else {
        cfg.export_height
    }
}

/// The overlay's viewport id. Fixed, so reopening it reuses the same window
/// rather than leaving a trail of dead ones — and derived from a hash, because
/// `ViewportId(Id::NULL)` is `ViewportId::ROOT`, the control window itself.
/// Pixel size of the overlay's spectrogram, once the lamps and the bearing
/// rose have taken what they need.
///
/// Lives here rather than on [`Config`] because the lamp column is measured
/// from the fonts, and fonts are a UI concern.
fn overlay_spectrogram_size(
    cfg: &crate::config::Config,
    overlay_width: f32,
    label_px: f32,
) -> (f32, f32) {
    if !cfg.overlay_spectrogram {
        return (0.0, 0.0);
    }
    let rose = if cfg.direction_finding {
        cfg.overlay_height
    } else {
        0.0
    };
    (
        (overlay_width - label_px - rose).max(16.0),
        cfg.overlay_height.max(16.0),
    )
}

/// Whether the overlay should be painted this frame.
///
/// Elite, and only Elite. An earlier version also showed it while *our* control
/// window had focus, so the toggles could be seen to act — but our window always
/// has focus the instant it opens, so the overlay appeared at startup and then
/// vanished a moment later. The overlay belongs to the cockpit; if you are not
/// looking at the cockpit there is nothing for it to annotate.
fn overlay_visible(game_found: bool, game_focused: bool) -> bool {
    game_found && game_focused
}

fn overlay_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("ed-compass-overlay")
}

fn anchor_from(cfg: &crate::config::Config) -> OverlayAnchor {
    OverlayAnchor {
        x_fraction: cfg.overlay_x_fraction,
        y_fraction: cfg.overlay_y_fraction,
        x_offset_px: cfg.overlay_x_offset_px,
        width: cfg.overlay_width,
        height: cfg.overlay_height,
    }
}

fn main_waterfall_interval(
    window_seconds: f32,
    width_px: f32,
    snapshot_interval: Duration,
    macos: bool,
) -> Duration {
    if !macos {
        return snapshot_interval;
    }
    let seconds_for_four_pixels = window_seconds * 4.0 / width_px.max(1.0);
    Duration::from_secs_f32(seconds_for_four_pixels.clamp(1.0 / 15.0, 0.25))
}

fn dragged_view_center(pointer_fraction: f32, grab_fraction: f32, box_width: f32) -> f32 {
    (pointer_fraction + (0.5 - grab_fraction) * box_width).clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelView {
    Combined,
    Single(usize),
    All,
}

#[derive(Debug, Clone, Copy)]
struct WaterfallLane {
    channel: Option<usize>,
    /// Reserved for later visual time alignment between channels.
    time_offset_seconds: f32,
}

struct CompassUi {
    app: App,
    snapshot: Option<AnalysisSnapshot>,
    last_snapshot: Instant,
    snapshot_interval: Duration,

    anchor: OverlayAnchor,
    /// Last answer from the window manager about where the game is and whether
    /// it has focus.
    placement: OverlayPlacement,
    /// The overlay's own spectrogram texture, kept separate from the full view's
    /// so switching views does not force a rebuild of either.
    /// The overlay's own texture, created and updated only inside the overlay
    /// viewport's pass. See [`overlay::OverlayState::spectrogram`] for why it
    /// cannot live in the main window's.
    overlay_texture: Arc<Mutex<Option<egui::TextureHandle>>>,
    /// What the overlay draws, shared with its viewport callback. The callback
    /// outlives any one frame, so the state it reads must too.
    overlay_state: Arc<Mutex<overlay::OverlayState>>,
    last_overlay_render: Instant,
    /// The newest spectrogram pixels, waiting to be handed to the overlay.
    pending_spectrogram: Option<egui::ColorImage>,
    /// Drives the overlay's displayed frequency band, so a detection is shown at
    /// a scale where it can be read. See [`zoom`].
    zoom: zoom::ZoomState,
    /// Width the lamp column needs, measured from the text it draws.
    overlay_label_px: f32,
    /// Whole-window alpha last applied to the overlay, once the Win32 call has
    /// actually succeeded. `None` until then, which keeps the off-screen
    /// fallback in play.
    overlay_alpha: Option<u8>,
    game_found: bool,
    last_game_poll: Instant,

    /// Where exported images go.
    export_dir: String,
    /// Editable journal directory. Kept in the UI until Apply is pressed so a
    /// half-typed path never disconnects a working watcher.
    journal_path: String,

    devices: Vec<AudioDevice>,
    waterfall_view: TimeViewport,
    /// Position within the viewport box grabbed for the current overview drag.
    /// `0` is its left edge, `1` its right; outside drags begin at the centre.
    overview_drag_grab_fraction: Option<f32>,
    overview_textures: Vec<Option<egui::TextureHandle>>,
    last_overview: Instant,
    overview_sizes: Vec<[usize; 2]>,
    waterfall_textures: Vec<Option<egui::TextureHandle>>,
    waterfall_sizes: Vec<[usize; 2]>,
    channel_view: ChannelView,
    /// Show the full retained spectrum instead of the detector-focused band.
    /// This is render-only, so it can be switched without restarting analysis.
    waterfall_full_spectrum: bool,
    last_waterfall: Instant,
    /// Size the waterfall image was last built at, so it is rebuilt on resize.
    /// When the per-frame logic last ran. See the check at the top of `logic`.
    last_logic: Instant,
    /// What the last Export did, and when, shown next to the button. A log line
    /// is no use to someone in a cockpit who needs to know the moment was saved.
    ///
    /// It expires rather than lingering. The message describes a moment that has
    /// passed, and the honest lifetime is exactly as long as it stays true: once
    /// the capture cooldown lapses, pressing Export would write a *new* file
    /// instead of reporting the old one, so the old text no longer describes what
    /// would happen and gets out of the way.
    export_status: Option<(String, Instant)>,
    setup_device_id: String,
    setup_library_path: String,
    setup_appearance: String,
    setup_error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct TimeViewport {
    max_seconds: f32,
    duration_seconds: f32,
    /// Absolute analysis time at the viewport's right edge. `None` follows now.
    inspected_end_seconds: Option<f64>,
}

impl TimeViewport {
    fn new(max_seconds: f32) -> Self {
        Self {
            max_seconds,
            duration_seconds: max_seconds,
            inspected_end_seconds: None,
        }
    }

    fn is_live(self) -> bool {
        self.inspected_end_seconds.is_none()
    }

    fn end_offset(self, now: f64) -> f32 {
        self.inspected_end_seconds
            .map(|end| (now - end).max(0.0) as f32)
            .unwrap_or(0.0)
    }

    fn set_duration(&mut self, now: f64, duration: f32) {
        let duration = duration.clamp(1.0, self.max_seconds);
        if duration >= self.max_seconds - f32::EPSILON {
            self.duration_seconds = self.max_seconds;
            self.inspected_end_seconds = None;
            return;
        }
        if let Some(old_end) = self.inspected_end_seconds {
            let center = old_end - self.duration_seconds as f64 * 0.5;
            let new_end = (center + duration as f64 * 0.5).min(now);
            self.inspected_end_seconds = (new_end < now - 0.001).then_some(new_end);
        }
        self.duration_seconds = duration;
    }

    fn inspect_age(&mut self, now: f64, age_seconds: f32) {
        if self.duration_seconds >= self.max_seconds - f32::EPSILON {
            self.inspected_end_seconds = None;
            return;
        }
        let center = now - age_seconds.clamp(0.0, self.max_seconds) as f64;
        let oldest_end = now - (self.max_seconds - self.duration_seconds) as f64;
        let end = (center + self.duration_seconds as f64 * 0.5).clamp(oldest_end, now);
        self.inspected_end_seconds = (end < now - 0.001).then_some(end);
    }

    fn update(&mut self, now: f64) {
        if self.end_offset(now) >= self.max_seconds {
            self.inspected_end_seconds = None;
        }
    }

    fn overview_range(self, now: f64) -> (f32, f32) {
        let offset = self.end_offset(now).clamp(0.0, self.max_seconds);
        let left = 1.0 - ((offset + self.duration_seconds) / self.max_seconds).clamp(0.0, 1.0);
        let right = 1.0 - (offset / self.max_seconds).clamp(0.0, 1.0);
        (left, right)
    }
}

impl CompassUi {
    fn new(app: App) -> Self {
        let interval = Duration::from_secs_f32(1.0 / app.config().analysis_update_hz.max(1.0));
        let anchor = anchor_from(app.config());
        let zoom = {
            let cfg = app.config();
            zoom::ZoomState::new(
                zoom::Band::new(cfg.spectrogram_min_hz, cfg.spectrogram_max_hz),
                cfg.overlay_zoom_hold_seconds,
                cfg.overlay_zoom_lockout_seconds,
                Instant::now(),
            )
        };
        let devices = device::enumerate().unwrap_or_default();
        let setup_device_id = if !app.config().device.is_empty() {
            app.config().device.clone()
        } else {
            devices
                .iter()
                .find(|device| device.name.eq_ignore_ascii_case("ED Compass Audio"))
                .or_else(|| {
                    devices
                        .iter()
                        .find(|device| device.name.to_ascii_lowercase().contains("ed compass"))
                })
                .map(|device| device.id.clone())
                .unwrap_or_default()
        };
        let setup_library_path = app.config().library_path.clone();
        let setup_appearance = app.config().appearance.clone();
        Self {
            anchor,
            placement: overlay_placement(anchor),
            overlay_texture: Arc::new(Mutex::new(None)),
            overlay_state: Arc::new(Mutex::new(overlay::OverlayState::default())),
            last_overlay_render: Instant::now() - Duration::from_secs(1),
            pending_spectrogram: None,
            zoom,
            overlay_label_px: 120.0,
            overlay_alpha: None,
            game_found: false,
            last_game_poll: Instant::now() - Duration::from_secs(10),
            snapshot_interval: interval,
            export_dir: app.config().export_dir.clone().unwrap_or_else(|| {
                if app.config().library_path.trim().is_empty() {
                    "exports".to_string()
                } else {
                    PathBuf::from(app.config().library_path.trim())
                        .join("Exports")
                        .display()
                        .to_string()
                }
            }),
            journal_path: if app.config().journal_path.is_empty() {
                crate::journal::JournalWatcher::default_dir()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            } else {
                app.config().journal_path.clone()
            },
            devices,
            waterfall_view: TimeViewport::new(app.config().waterfall_seconds),
            overview_drag_grab_fraction: None,
            overview_textures: Vec::new(),
            last_overview: Instant::now() - Duration::from_secs(1),
            overview_sizes: Vec::new(),
            app,
            snapshot: None,
            last_snapshot: Instant::now() - Duration::from_secs(1),
            waterfall_textures: Vec::new(),
            waterfall_sizes: Vec::new(),
            channel_view: ChannelView::Combined,
            waterfall_full_spectrum: false,
            last_waterfall: Instant::now() - Duration::from_secs(1),
            last_logic: Instant::now(),
            export_status: None,
            setup_device_id,
            setup_library_path,
            setup_appearance,
            setup_error: None,
        }
    }

    fn first_launch_setup(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                // Centre the form as one compact unit. Previously only these
                // two labels were centred while the grid started at the
                // panel's left edge, so they appeared unrelated on a wide Mac
                // window.
                ui.set_width(650.0);
                ui.add_space(20.0);
                ui.heading("Set up ED Compass");
                ui.label("These defaults are ready to use. Review them, then continue.");
                ui.add_space(20.0);
                egui::Grid::new("first-launch-settings")
                    .num_columns(2)
                    .spacing([16.0, 14.0])
                    .show(ui, |ui| {
                        ui.label("Audio input");
                        let selected = self
                            .devices
                            .iter()
                            .find(|device| device.id == self.setup_device_id)
                            .map(AudioDevice::display_name)
                            .unwrap_or_else(|| "Choose a Loopback device".into());
                        egui::ComboBox::from_id_salt("setup-device")
                            .selected_text(selected)
                            .width(480.0)
                            .show_ui(ui, |ui| {
                                for device in &self.devices {
                                    ui.selectable_value(
                                        &mut self.setup_device_id,
                                        device.id.clone(),
                                        device.display_name(),
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label("Capture library");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.setup_library_path)
                                .desired_width(480.0),
                        );
                        ui.end_row();

                        ui.label("Journal directory");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.journal_path).desired_width(480.0),
                        );
                        ui.end_row();

                        ui.label("Appearance");
                        egui::ComboBox::from_id_salt("setup-appearance")
                            .selected_text(match self.setup_appearance.as_str() {
                                "light" => "Light",
                                "dark" => "Dark",
                                _ => "System",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.setup_appearance,
                                    "system".into(),
                                    "System",
                                );
                                ui.selectable_value(
                                    &mut self.setup_appearance,
                                    "light".into(),
                                    "Light",
                                );
                                ui.selectable_value(
                                    &mut self.setup_appearance,
                                    "dark".into(),
                                    "Dark",
                                );
                            });
                        ui.end_row();
                    });

                ui.add_space(16.0);
                if let Some(error) = &self.setup_error {
                    ui.colored_label(egui::Color32::from_rgb(220, 70, 70), error);
                }
                ui.horizontal(|ui| {
                    if ui.button("Re-scan audio inputs").clicked() {
                        self.devices = device::enumerate().unwrap_or_default();
                    }
                    if ui.button("Continue").clicked() {
                        self.setup_error = self
                            .finish_setup(ctx)
                            .err()
                            .map(|error| format!("{error:#}"));
                    }
                });
            });
        });
    }

    fn finish_setup(&mut self, ctx: &egui::Context) -> Result<()> {
        let chosen = self
            .devices
            .iter()
            .find(|device| device.id == self.setup_device_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("choose the Loopback audio input"))?;
        self.app.switch_device(&chosen)?;
        self.app.set_journal_path(self.journal_path.clone());
        self.app.complete_setup(
            self.setup_library_path.clone(),
            self.setup_appearance.clone(),
        )?;
        self.export_dir = PathBuf::from(self.setup_library_path.trim())
            .join("Exports")
            .display()
            .to_string();
        apply_appearance(ctx, &self.setup_appearance);
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
            900.0, 620.0,
        )));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1180.0, 860.0)));
        Ok(())
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("ED Compass");
            ui.add_space(12.0);
            let status = self.app.status();
            ui.label(
                egui::RichText::new(status.label())
                    .monospace()
                    .size(15.0)
                    .color(controls::status_colour(status)),
            );
            if status == Status::Warming {
                ui.add(
                    egui::ProgressBar::new(self.app.warmup_progress())
                        .desired_width(120.0)
                        .show_percentage(),
                )
                .on_hover_text(
                    "The detector is learning what the background looks like. \
                     Detection is suppressed until it settles.",
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(e) = self.app.error() {
                    ui.label(
                        egui::RichText::new(e)
                            .monospace()
                            .color(egui::Color32::from_rgb(255, 110, 110)),
                    );
                }
            });
        });

        ui.horizontal(|ui| {
            ui.label("Device:");
            let current = self.app.device_label().to_owned();
            if let Some(device) = controls::device_picker(ui, &self.devices, &current)
                && let Err(e) = self.app.switch_device(&device)
            {
                log::error!("could not switch device: {e:#}");
            }
            if ui.button("↻").on_hover_text("Re-scan endpoints").clicked() {
                self.devices = device::enumerate().unwrap_or_default();
            }
            let mut excess = self.app.config().spectrogram_show_excess;
            if ui
                .checkbox(&mut excess, "excess")
                .on_hover_text(
                    "Show each bin minus its learned background. Removes anything \
                     constantly loud — ship rumble, life support — and leaves only \
                     what changed.",
                )
                .changed()
            {
                self.app.set_show_excess(excess);
                self.last_overview = Instant::now() - Duration::from_secs(1);
                self.last_waterfall = Instant::now() - Duration::from_secs(1);
            }
            if ui
                .checkbox(&mut self.waterfall_full_spectrum, "full spectrum")
                .on_hover_text(
                    "Show the full 20 Hz–24 kHz spectral display. Off keeps the focused 200–2400 Hz signal band.",
                )
                .changed()
            {
                self.last_overview = Instant::now() - Duration::from_secs(1);
                self.last_waterfall = Instant::now() - Duration::from_secs(1);
            }
            if ui
                .button("Export")
                .on_hover_text(
                    "Keep everything about this moment: the recent audio, and the \
                     spectrogram as an image. Use it the instant you see something, \
                     whether or not the lamps agree.",
                )
                .clicked()
            {
                self.export_everything();
            }

            ui.separator();
            match self.app.format() {
                Some(f) => {
                    ui.label(egui::RichText::new(f.describe()).monospace());
                    let directional = f.directional_channels();
                    if directional < 3 {
                        ui.label(
                            egui::RichText::new(format!("{directional} directional ch"))
                                .monospace()
                                .color(egui::Color32::from_rgb(255, 210, 90)),
                        )
                        .on_hover_text(
                            "Set the Windows output endpoint to 7.1 for a far sharper bearing. \
                             It works on a stereo headset — it is the endpoint mix format that \
                             matters, not the hardware.",
                        );
                    }
                }
                None => {
                    ui.weak("waiting for the stream…");
                }
            }
        });

        ui.horizontal(|ui| {
            let game = self.app.game_state();
            ui.label("System:");
            ui.label(egui::RichText::new(game.describe()).monospace());
            if let Some(track) = &game.music_track {
                ui.separator();
                ui.weak(egui::RichText::new(format!("music: {track}")).monospace())
                    .on_hover_text(
                        "A detection coinciding with a music change is a prime \
                         false-positive suspect.",
                    );
            }
            if let Some(snap) = &self.snapshot {
                let capture_health = self.app.capture_health();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(health) = capture_health {
                        let colour = if health.dropped_frames == 0 {
                            egui::Color32::from_gray(120)
                        } else {
                            egui::Color32::from_rgb(255, 150, 90)
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "audio queue: {} drops / {} frames",
                                health.queue_full_events, health.dropped_frames
                            ))
                            .monospace()
                            .color(colour),
                        )
                        .on_hover_text(format!(
                            "{} callbacks · {} input frames · {} delivered frames · largest callback {} frames · {} device-gap frames",
                            health.callbacks,
                            health.input_frames,
                            health.delivered_frames,
                            health.largest_callback_frames,
                            health.device_gap_frames,
                        ));
                        ui.separator();
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{:.0} s analyzed · {} gaps ({:.1} s) · {} captures",
                            snap.timeline_seconds,
                            snap.gap_count,
                            snap.gap_seconds,
                            self.app.captures_written()
                        ))
                        .monospace()
                        .color(egui::Color32::from_gray(150)),
                    );
                });
            }
        });

        ui.horizontal(|ui| {
            ui.label("Journal:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.journal_path)
                    .desired_width(ui.available_width() - 70.0)
                    .hint_text("Elite Dangerous journal directory"),
            );
            let apply = ui.button("Apply").clicked()
                || (response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
            if apply {
                self.app.set_journal_path(self.journal_path.clone());
            }
            ui.separator();
            ui.label("Appearance:");
            let before = self.setup_appearance.clone();
            egui::ComboBox::from_id_salt("appearance")
                .selected_text(match self.setup_appearance.as_str() {
                    "light" => "Light",
                    "dark" => "Dark",
                    _ => "System",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.setup_appearance, "system".into(), "System");
                    ui.selectable_value(&mut self.setup_appearance, "light".into(), "Light");
                    ui.selectable_value(&mut self.setup_appearance, "dark".into(), "Dark");
                });
            if self.setup_appearance != before {
                apply_appearance(ui.ctx(), &self.setup_appearance);
                self.app.set_appearance(self.setup_appearance.clone());
            }
        });
    }

    fn waterfall_scale(
        &self,
        geometry: crate::analysis::novelty::FrameGeometry,
    ) -> waterfall::FreqScale {
        let cfg = self.app.config();
        let (min_hz, max_hz) = if self.waterfall_full_spectrum {
            (waterfall::DEFAULT_MIN_HZ, waterfall::FULL_SPECTRUM_MAX_HZ)
        } else {
            (cfg.spectrogram_min_hz, cfg.spectrogram_max_hz)
        };
        waterfall::FreqScale::new(min_hz, max_hz, geometry.nyquist_hz())
    }

    fn channel_lanes(&self) -> Vec<WaterfallLane> {
        let channels = self.app.format().map_or(0, |format| format.channels);
        match self.channel_view {
            ChannelView::Combined => vec![WaterfallLane {
                channel: None,
                time_offset_seconds: 0.0,
            }],
            ChannelView::Single(channel) if channel < channels => vec![WaterfallLane {
                channel: Some(channel),
                time_offset_seconds: 0.0,
            }],
            ChannelView::All if channels > 1 => (0..channels)
                .map(|channel| WaterfallLane {
                    channel: Some(channel),
                    time_offset_seconds: 0.0,
                })
                .collect(),
            _ => vec![WaterfallLane {
                channel: None,
                time_offset_seconds: 0.0,
            }],
        }
    }

    fn channel_label(&self, channel: Option<usize>) -> String {
        let Some(channel) = channel else {
            return "Combined".into();
        };
        self.app
            .format()
            .and_then(|format| format.layout().get(channel).copied())
            .map(|info| {
                if info.name == "?" {
                    format!("Channel {}", channel + 1)
                } else {
                    info.name.to_owned()
                }
            })
            .unwrap_or_else(|| format!("Channel {}", channel + 1))
    }

    fn channel_view_controls(&mut self, ui: &mut egui::Ui) {
        let Some(format) = self.app.format() else {
            return;
        };
        let channels = format.channels;
        if matches!(self.channel_view, ChannelView::Single(channel) if channel >= channels)
            || matches!(self.channel_view, ChannelView::All if channels < 2)
        {
            self.channel_view = ChannelView::Combined;
        }
        let selected = match self.channel_view {
            ChannelView::Combined => "Combined".into(),
            ChannelView::Single(channel) => self.channel_label(Some(channel)),
            ChannelView::All if channels == 2 => "L + R".into(),
            ChannelView::All => "All channels".into(),
        };
        let before = self.channel_view;
        ui.label("Channels:");
        egui::ComboBox::from_id_salt("waterfall-channel-view")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.channel_view, ChannelView::Combined, "Combined");
                for (channel, info) in format.layout().iter().enumerate() {
                    let label = if info.name == "?" {
                        format!("Channel {}", channel + 1)
                    } else {
                        info.name.to_owned()
                    };
                    ui.selectable_value(
                        &mut self.channel_view,
                        ChannelView::Single(channel),
                        label,
                    );
                }
                if channels > 1 {
                    ui.selectable_value(
                        &mut self.channel_view,
                        ChannelView::All,
                        if channels == 2 {
                            "L + R"
                        } else {
                            "All channels"
                        },
                    );
                }
            });
        if self.channel_view != before {
            self.last_overview = Instant::now() - Duration::from_secs(1);
            self.last_waterfall = Instant::now() - Duration::from_secs(1);
        }
    }

    fn waterfall_overview(&mut self, ui: &mut egui::Ui) {
        let now = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.timeline_seconds)
            .unwrap_or(0.0);

        ui.horizontal(|ui| {
            self.channel_view_controls(ui);
            ui.separator();
            ui.label(egui::RichText::new("TIME").monospace().size(11.0));
            for seconds in [140.0f32, 70.0, 35.0, 15.0] {
                let selected = (self.waterfall_view.duration_seconds - seconds).abs() < 0.1;
                if ui
                    .selectable_label(selected, format!("{seconds:.0}s"))
                    .clicked()
                {
                    self.waterfall_view.set_duration(now, seconds);
                    self.last_waterfall = Instant::now() - Duration::from_secs(1);
                }
            }
            if ui
                .add_enabled(!self.waterfall_view.is_live(), egui::Button::new("Live"))
                .clicked()
            {
                self.waterfall_view.inspected_end_seconds = None;
                self.last_waterfall = Instant::now() - Duration::from_secs(1);
            }
            if !self.waterfall_view.is_live() {
                ui.weak(format!(
                    "inspecting · right edge -{:.0}s",
                    self.waterfall_view.end_offset(now)
                ));
            }
        });

        let lanes = self.channel_lanes();
        let lane_height = if lanes.len() == 1 { 108.0 } else { 82.0 };
        let width = ui.available_width();
        let (response, painter) = ui.allocate_painter(
            egui::vec2(width, lane_height * lanes.len() as f32),
            egui::Sense::click_and_drag(),
        );
        let rect = response.rect;
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(12));

        let Some(engine) = self.app.engine() else {
            return;
        };
        let geometry = engine.geometry();
        let scale = self.waterfall_scale(geometry);
        let cfg = self.app.config();
        let max_seconds = self.waterfall_view.max_seconds;
        let interval = if cfg!(target_os = "macos") {
            self.snapshot_interval.max(Duration::from_millis(250))
        } else {
            self.snapshot_interval
        };
        let refresh = self.last_overview.elapsed() >= interval;
        self.overview_textures.resize_with(lanes.len(), || None);
        self.overview_sizes.resize(lanes.len(), [0, 0]);
        for (lane_index, lane) in lanes.iter().enumerate() {
            let lane_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() + lane_index as f32 * lane_height),
                egui::pos2(
                    rect.right(),
                    rect.top() + (lane_index + 1) as f32 * lane_height,
                ),
            );
            let target = [lane_rect.width() as usize, lane_rect.height() as usize];
            let history = match (cfg.spectrogram_show_excess, lane.channel) {
                (true, Some(channel)) => engine.channel_excess_waterfall(channel),
                (false, Some(channel)) => engine.channel_waterfall(channel),
                (true, None) => Some(engine.excess_waterfall()),
                (false, None) => Some(engine.waterfall()),
            };
            let Some(history) = history else { continue };
            if self.overview_textures[lane_index].is_none()
                || target != self.overview_sizes[lane_index]
                || refresh
            {
                let window_frames =
                    (max_seconds / geometry.frame_seconds()).ceil().max(1.0) as usize;
                let mut image = waterfall::build_image(
                    history,
                    geometry,
                    waterfall::RenderOptions {
                        scale,
                        auto_gain: true,
                        median_subtract: cfg.spectrogram_median_subtract,
                        window_frames,
                        end_offset_frames: 0,
                    },
                    target[0],
                    target[1],
                );
                let slices = self.timeline_slices(max_seconds, target[0]);
                waterfall::paint_timeline(&mut image, &slices);
                match &mut self.overview_textures[lane_index] {
                    Some(handle) => handle.set(image, egui::TextureOptions::NEAREST),
                    None => {
                        self.overview_textures[lane_index] = Some(ui.ctx().load_texture(
                            format!("waterfall-overview-{lane_index}"),
                            image,
                            egui::TextureOptions::NEAREST,
                        ));
                    }
                }
                self.overview_sizes[lane_index] = target;
            }

            if let Some(texture) = &self.overview_textures[lane_index] {
                painter.image(
                    texture.id(),
                    lane_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            waterfall::draw_axes(&painter, lane_rect, scale, max_seconds, 0.0);
            painter.text(
                egui::pos2(lane_rect.right() - 6.0, lane_rect.top() + 5.0),
                egui::Align2::RIGHT_TOP,
                self.channel_label(lane.channel),
                egui::FontId::monospace(11.0),
                egui::Color32::WHITE,
            );
            for stroke in engine.traced_strokes() {
                waterfall::draw_event_box(
                    &painter,
                    lane_rect,
                    scale,
                    max_seconds,
                    0.0,
                    waterfall::EventBox {
                        seconds_ago_start: (now - stroke.start_seconds) as f32,
                        seconds_ago_end: (now - stroke.end_seconds) as f32,
                        low_hz: stroke.low_hz,
                        high_hz: stroke.high_hz,
                        captured: false,
                        traced: true,
                        subdued: lane.channel.is_some(),
                    },
                );
            }
            for record in self.app.events().iter().rev() {
                let event = &record.detection.event;
                let ago_start = (now - event.start_seconds) as f32;
                waterfall::draw_event_box(
                    &painter,
                    lane_rect,
                    scale,
                    max_seconds,
                    0.0,
                    waterfall::EventBox {
                        seconds_ago_start: ago_start,
                        seconds_ago_end: ago_start - event.duration_seconds,
                        low_hz: event.low_hz,
                        high_hz: event.high_hz,
                        captured: record.captured_to.is_some(),
                        traced: false,
                        subdued: lane.channel.is_some(),
                    },
                );
            }
        }
        if refresh {
            self.last_overview = Instant::now();
        }

        let (left, right) = self.waterfall_view.overview_range(now);
        let viewport = egui::Rect::from_min_max(
            egui::pos2(rect.left() + rect.width() * left, rect.top()),
            egui::pos2(rect.left() + rect.width() * right, rect.bottom()),
        );
        if self.waterfall_view.duration_seconds < max_seconds - f32::EPSILON {
            painter.rect_filled(
                egui::Rect::from_min_max(rect.min, egui::pos2(viewport.left(), rect.bottom())),
                0.0,
                egui::Color32::from_black_alpha(90),
            );
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(viewport.right(), rect.top()), rect.max),
                0.0,
                egui::Color32::from_black_alpha(90),
            );
            painter.rect_stroke(
                viewport,
                1.0,
                egui::Stroke::new(2.0, egui::Color32::WHITE),
                egui::StrokeKind::Inside,
            );
        }

        if self.waterfall_view.duration_seconds < max_seconds - f32::EPSILON {
            let pointer_fraction =
                |pointer: egui::Pos2| ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            if response.hovered() {
                let cursor = if response.dragged() {
                    egui::CursorIcon::Grabbing
                } else if response
                    .hover_pos()
                    .map(pointer_fraction)
                    .is_some_and(|fraction| fraction >= left && fraction <= right)
                {
                    egui::CursorIcon::Grab
                } else {
                    egui::CursorIcon::PointingHand
                };
                ui.ctx().set_cursor_icon(cursor);
            }

            if response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let fraction = pointer_fraction(pointer);
                if fraction < left || fraction > right {
                    self.waterfall_view
                        .inspect_age(now, max_seconds * (1.0 - fraction));
                    self.last_waterfall = Instant::now() - Duration::from_secs(1);
                }
            }

            if response.drag_started() {
                let pressed = ui
                    .ctx()
                    .input(|input| input.pointer.press_origin())
                    .or_else(|| response.interact_pointer_pos());
                self.overview_drag_grab_fraction = pressed.map(|pointer| {
                    let fraction = pointer_fraction(pointer);
                    if fraction >= left && fraction <= right {
                        ((fraction - left) / (right - left).max(f32::EPSILON)).clamp(0.0, 1.0)
                    } else {
                        0.5
                    }
                });
            }
            if response.dragged()
                && let (Some(pointer), Some(grab)) = (
                    response.interact_pointer_pos(),
                    self.overview_drag_grab_fraction,
                )
            {
                let box_width = self.waterfall_view.duration_seconds / max_seconds;
                let center_fraction =
                    dragged_view_center(pointer_fraction(pointer), grab, box_width);
                self.waterfall_view
                    .inspect_age(now, max_seconds * (1.0 - center_fraction));
                self.last_waterfall = Instant::now() - Duration::from_secs(1);
            }
            if response.drag_stopped() {
                self.overview_drag_grab_fraction = None;
            }
        }
    }

    fn waterfall_panels(&mut self, ui: &mut egui::Ui, viewport_height: f32) {
        let lanes = self.channel_lanes();
        let available = ui.available_size();
        let height = if lanes.len() == 1 {
            (viewport_height - 240.0).max(180.0)
        } else {
            260.0
        };
        let refresh = self.last_waterfall.elapsed()
            >= main_waterfall_interval(
                self.waterfall_view.duration_seconds,
                available.x,
                self.snapshot_interval,
                cfg!(target_os = "macos"),
            );
        self.waterfall_textures.resize_with(lanes.len(), || None);
        self.waterfall_sizes.resize(lanes.len(), [0, 0]);
        for (lane_index, lane) in lanes.iter().enumerate() {
            if lane_index > 0 {
                ui.add_space(6.0);
            }
            self.waterfall_lane(ui, *lane, lane_index, height, refresh);
        }
        if refresh {
            self.last_waterfall = Instant::now();
        }
    }

    fn waterfall_lane(
        &mut self,
        ui: &mut egui::Ui,
        lane: WaterfallLane,
        lane_index: usize,
        height: f32,
        refresh: bool,
    ) {
        let available = ui.available_size();
        let size = egui::vec2(available.x, height);
        let (response, painter) = ui.allocate_painter(size, egui::Sense::click_and_drag());
        let rect = response.rect;

        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(12));

        let Some(engine) = self.app.engine() else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "waiting for audio…",
                egui::FontId::monospace(13.0),
                egui::Color32::from_gray(120),
            );
            return;
        };
        let geometry = engine.geometry();
        let cfg = self.app.config();
        let scale = self.waterfall_scale(geometry);
        let window_seconds = self.waterfall_view.duration_seconds;
        let now_seconds = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.timeline_seconds)
            .unwrap_or(0.0);
        let end_offset_seconds =
            (self.waterfall_view.end_offset(now_seconds) + lane.time_offset_seconds).max(0.0);

        // Rebuilding the image is the expensive part, so it runs at the
        // snapshot rate rather than the frame rate.
        let target = [rect.width() as usize, rect.height() as usize];
        // Bound scroll jumps in screen pixels. Four rebuilds per second are
        // smooth across 140 seconds, but the same cadence jumps roughly 17 px
        // at a 15-second zoom on a 1000 px view. Short views therefore update
        // faster while the long baseline retains its measured low-cost cadence.
        if self.waterfall_textures[lane_index].is_none()
            || target != self.waterfall_sizes[lane_index]
            || refresh
        {
            let history = match (cfg.spectrogram_show_excess, lane.channel) {
                (true, Some(channel)) => engine.channel_excess_waterfall(channel),
                (false, Some(channel)) => engine.channel_waterfall(channel),
                (true, None) => Some(engine.excess_waterfall()),
                (false, None) => Some(engine.waterfall()),
            };
            let Some(history) = history else { return };
            // The window in frames, so time-per-pixel is fixed and the display
            // scrolls at a constant rate from the first second.
            let window_frames =
                (window_seconds / geometry.frame_seconds()).ceil().max(1.0) as usize;
            let end_offset_frames =
                (end_offset_seconds / geometry.frame_seconds()).round() as usize;
            let image = waterfall::build_image(
                history,
                geometry,
                waterfall::RenderOptions {
                    scale,
                    auto_gain: true,
                    median_subtract: cfg.spectrogram_median_subtract,
                    window_frames,
                    end_offset_frames,
                },
                target[0],
                target[1],
            );
            // The timeline goes into the same buffer, so it scrolls with the
            // rows it describes rather than on its own clock.
            let mut image = image;
            // One slice per pixel. At three the strip could only move in
            // three-pixel jumps while the spectrogram beneath it moved one at a
            // time, and the mismatch is exactly what reads as juddering once the
            // strip is bright enough to notice.
            let slices = self.timeline_slices_ending_at(
                now_seconds - end_offset_seconds as f64,
                window_seconds,
                target[0],
            );
            waterfall::paint_timeline(&mut image, &slices);
            // Update in place. Assigning a fresh `load_texture` here dropped
            // the old handle, which queues a *free* into egui's global texture
            // delta — about eight per second at the snapshot rate. Two
            // viewports drain that one queue on independent schedules, so a
            // free could be applied by one painter while the other still had
            // the id in its draw list: "Texture with 'egui_texid_Managed(7833)'
            // label is invalid", and the process died. `set` keeps one id for
            // the life of the process and queues no frees at all.
            match &mut self.waterfall_textures[lane_index] {
                Some(handle) => handle.set(image, egui::TextureOptions::NEAREST),
                None => {
                    self.waterfall_textures[lane_index] = Some(ui.ctx().load_texture(
                        format!("waterfall-{lane_index}"),
                        image,
                        egui::TextureOptions::NEAREST,
                    ));
                }
            }
            self.waterfall_sizes[lane_index] = target;
        }

        if let Some(texture) = &self.waterfall_textures[lane_index] {
            painter.image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        waterfall::draw_axes(&painter, rect, scale, window_seconds, end_offset_seconds);
        painter.text(
            egui::pos2(rect.right() - 8.0, rect.top() + 7.0),
            egui::Align2::RIGHT_TOP,
            self.channel_label(lane.channel),
            egui::FontId::monospace(12.0),
            egui::Color32::WHITE,
        );

        // One clock for both kinds of outline, so they age together.
        // Strokes the tracer followed. Drawn first, so a detection box sits on
        // top when the two describe the same thing.
        if let Some(engine) = self.app.engine() {
            for stroke in engine.traced_strokes() {
                // Aged exactly like a detection box, from the same clock.
                let ago_start = (now_seconds - stroke.start_seconds) as f32;
                let ago_end = (now_seconds - stroke.end_seconds) as f32;
                waterfall::draw_event_box(
                    &painter,
                    rect,
                    scale,
                    window_seconds,
                    end_offset_seconds,
                    waterfall::EventBox {
                        seconds_ago_start: ago_start,
                        seconds_ago_end: ago_end.max(0.0),
                        low_hz: stroke.low_hz,
                        high_hz: stroke.high_hz,
                        captured: false,
                        traced: true,
                        subdued: lane.channel.is_some(),
                    },
                );
            }
        }

        for record in self.app.events().iter().rev() {
            let e = &record.detection.event;
            let ago_start = (now_seconds - e.start_seconds) as f32;
            let ago_end = ago_start - e.duration_seconds;
            waterfall::draw_event_box(
                &painter,
                rect,
                scale,
                window_seconds,
                end_offset_seconds,
                waterfall::EventBox {
                    seconds_ago_start: ago_start,
                    seconds_ago_end: ago_end.max(0.0),
                    low_hz: e.low_hz,
                    high_hz: e.high_hz,
                    captured: record.captured_to.is_some(),
                    traced: false,
                    subdued: lane.channel.is_some(),
                },
            );
        }

        // Drag vertically to mute a frequency range.
        if response.drag_stopped()
            && let Some(origin) = response.interact_pointer_pos()
        {
            let height = rect.height() as usize;
            let a = scale.hz((origin.y - rect.top()).max(0.0) as usize, height);
            let delta = response.drag_delta().y;
            let b = scale.hz((origin.y - delta - rect.top()).max(0.0) as usize, height);
            if (a - b).abs() > 1.0 {
                self.app.mute_band(a.min(b), a.max(b));
            }
        }
    }

    /// The two primary readouts: is something transmitting, and is something
    /// drawn. These lead because they are what the tool is for.
    /// The arming controls, inherited from the retired compact panel.
    ///
    /// Each is read back from the app so the widget can never drift out of step
    /// with what is actually running.
    fn controls_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Bound to "listening", which is the inverse of paused — binding
            // the checkbox straight to `paused` would have made ticking it
            // stop analysis.
            let mut listening = !self.app.is_paused();
            if ui
                .checkbox(&mut listening, "Listening")
                .on_hover_text("Unchecked suspends analysis. The audio device stays open.")
                .changed()
            {
                self.app.set_paused(!listening);
            }

            let mut keying = self.app.detect_keying();
            let mut structure = self.app.detect_structure();
            let a = ui.checkbox(&mut keying, "Detect transmissions").changed();
            let b = ui.checkbox(&mut structure, "Detect pictures").changed();
            if a || b {
                self.app.set_detectors(keying, structure);
            }

            #[cfg(windows)]
            {
                let mut overlay_on = self.app.overlay_enabled();
                if ui
                    .checkbox(&mut overlay_on, "In-game overlay")
                    .on_hover_text(
                        "Shows the indicators over the cockpit whenever Elite has \
                         focus, and hides them again when it does not. This window \
                         stays open either way.",
                    )
                    .changed()
                {
                    self.app.set_overlay_enabled(overlay_on);
                }
                if overlay_on && !self.game_found {
                    ui.label(
                        egui::RichText::new("waiting for the Elite Dangerous window")
                            .monospace()
                            .size(10.0)
                            .color(overlay::hud::AMBER),
                    );
                }
            }

            let mut df = self.app.direction_finding();
            if ui
                .checkbox(&mut df, "Direction finding")
                .on_hover_text(
                    "Secondary bearing analysis. Keeps every channel in the capture \
                     ring. Switching it rebuilds the analysis engine and loses history.",
                )
                .changed()
            {
                self.app.set_direction_finding(df);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Shown only while it still describes what another press would
                // do. See `export_status`.
                let linger =
                    Duration::from_secs_f32(self.app.config().capture_cooldown_seconds.max(1.0));
                match &self.export_status {
                    Some((status, at)) if at.elapsed() < linger => {
                        ui.label(egui::RichText::new(status).weak());
                    }
                    Some(_) => self.export_status = None,
                    None => {}
                }
            });
        });
    }

    /// Show or hide the in-game overlay, following Elite's focus.
    ///
    /// A deferred viewport rather than a mode of this window: the control panel
    /// stays open and reachable at all times, and the overlay simply is not
    /// there when the game is not in front of you. Not calling
    /// `show_viewport_deferred` in a frame closes the window, which is the whole
    /// hide mechanism.
    /// Keep the in-game overlay in step with the game window.
    ///
    /// The window is created once and **never destroyed**. When the overlay
    /// should not be seen it stays open and paints nothing — a transparent
    /// window with nothing in it is invisible, and it is already click-through
    /// and absent from the taskbar, so an empty one costs the player nothing.
    ///
    /// Both obvious alternatives are broken, each in its own way:
    ///
    /// * **Destroying it** (not calling `show_viewport_deferred`) churns
    ///   viewport lifecycle on every focus change, and `egui_wgpu`'s
    ///   `Painter::set_window(id, None)` does `self.surfaces.clear()` — *all*
    ///   surfaces, not just that viewport's. Tearing down the overlay can take
    ///   the main window's surface with it, and the textures that were valid
    ///   against it: "Texture with 'egui_texid_Managed(1)' label is invalid".
    /// * **Hiding it** with `ViewportBuilder::with_visible(false)` freezes it:
    ///   a hidden window receives no redraw events, so it never renders again,
    ///   and `Visible(true)` does not restart its render loop.
    ///
    /// Painting nothing has neither problem: no lifecycle churn, and the render
    /// loop never stops.
    fn sync_overlay(&mut self, ctx: &egui::Context) {
        // `cfg!` deliberately leaves the Windows body type-checked on macOS,
        // while this runtime-constant return guarantees that no game-window
        // polling or secondary viewport occurs there.
        if !cfg!(windows) {
            return;
        }
        // Only ask the window manager occasionally for the rectangle — the
        // player is not moving the game window every frame — but a quarter of a
        // second is fast enough that an Alt-Tab feels immediate.
        if self.last_game_poll.elapsed() >= Duration::from_millis(250) {
            self.last_game_poll = Instant::now();
            // Two passes: the first learns how wide the game window is, the
            // second places an overlay sized to the band SrvSurvey leaves free
            // inside it. Cheap — both are arithmetic on a cached rectangle.
            let probe = overlay_placement(self.anchor);
            if self.app.config().overlay_fit_between_plotters
                && let Some((x, width)) = PlotterGap::default().band(probe.game_width)
            {
                self.anchor.width = width;
                self.anchor.x_fraction = 0.0;
                self.anchor.x_offset_px = x;
            }
            self.placement = overlay_placement(self.anchor);
        }
        let placement = self.placement;
        self.game_found = placement.game_found;

        let showing = self.app.overlay_enabled()
            && overlay_visible(placement.game_found, placement.game_focused);

        if showing {
            // Measure the column before the image is sized: the spectrogram
            // gets whatever the lamps do not need.
            let probe = overlay::OverlayState::from_app(&self.app);
            self.overlay_label_px = overlay::label_column_width(ctx, &probe);
            self.rebuild_overlay_spectrogram(ctx);
        }

        // Published through a shared cell rather than captured by value. egui
        // may render the child while the parent sleeps, and its docs call for
        // exactly this; a closure capturing a snapshot can only ever show the
        // state of the frame that built it.
        // Computed before the shared state is locked: it needs `&mut self` for
        // the zoom, and the lock borrows self immutably.
        let strokes = self.overlay_strokes();
        let animating = self.zoom.animating();
        let timeline = self.overlay_timeline();
        {
            let mut shared = self.overlay_state.lock().unwrap();
            let pixels = shared.spectrogram.take();
            *shared = overlay::OverlayState::from_app(&self.app);
            shared.showing = showing;
            // Carry forward any frame the overlay has not consumed yet, so a
            // slow child never loses the newest image it was handed.
            shared.spectrogram = self
                .pending_spectrogram
                .take()
                .or(pixels)
                .filter(|_| self.app.config().overlay_spectrogram);
            shared.direction = self
                .snapshot
                .as_ref()
                .map(|s| s.direction)
                .filter(|_| self.app.config().direction_finding);
            shared.strokes = strokes;
            shared.animating = animating;
            shared.timeline = timeline;
        }

        // Hidden by opacity, the way SrvSurvey hides its plotters: the window
        // stays open, in place, and rendering, but composites to nothing.
        //
        // Position is the fallback, and only the fallback. If the Win32 call
        // has not succeeded yet — the window does not exist on the first frame
        // — the overlay is parked off-screen instead, so there is never a
        // moment where it is both wanted-hidden and visible.
        let hidden_by_opacity = self.overlay_alpha == Some(0);
        let position = if showing || hidden_by_opacity {
            placement.position
        } else {
            crate::game_window::PARKED_POSITION
        };

        let builder = egui::ViewportBuilder::default()
            .with_title(crate::game_window::OVERLAY_WINDOW_TITLE)
            .with_position([position.0, position.1])
            .with_inner_size([self.anchor.width, self.anchor.height])
            .with_decorations(false)
            .with_transparent(true)
            // Click-through, so it can never steal a click meant for the cockpit.
            .with_mouse_passthrough(true)
            // Never take focus, not even on creation.
            .with_active(false)
            // No taskbar entry and no Alt-Tab stop: it is not a window you are
            // ever meant to interact with, and it cannot be lost behind
            // anything because the control window owns the process.
            .with_taskbar(false)
            .with_always_on_top();

        let wanted_alpha = if showing { 255 } else { 0 };
        if self.overlay_alpha != Some(wanted_alpha)
            && crate::game_window::set_overlay_opacity(
                crate::game_window::OVERLAY_WINDOW_TITLE,
                wanted_alpha,
            )
        {
            log::debug!("overlay opacity set to {wanted_alpha}");
            self.overlay_alpha = Some(wanted_alpha);
        }

        let state = Arc::clone(&self.overlay_state);
        let texture = Arc::clone(&self.overlay_texture);
        ctx.show_viewport_deferred(overlay_viewport_id(), builder, move |ctx, _class| {
            // Take the newest pixels and upload them here, in the overlay's own
            // pass, so this texture is allocated, written and drawn by exactly
            // one viewport.
            let snapshot = {
                let mut shared = state.lock().unwrap();
                let pixels = shared.spectrogram.take();
                let snapshot = shared.clone();
                drop(shared);
                if let Some(image) = pixels {
                    let mut texture = texture.lock().unwrap();
                    match &mut *texture {
                        Some(handle) => handle.set(image, egui::TextureOptions::NEAREST),
                        None => {
                            *texture = Some(ctx.load_texture(
                                "overlay-spectrogram",
                                image,
                                egui::TextureOptions::NEAREST,
                            ));
                        }
                    }
                }
                snapshot
            };

            // No frame and no background: whatever is not painted stays
            // transparent and the cockpit shows through. When not showing we
            // paint nothing at all, which is what makes the window invisible
            // without ever having to close it.
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    if snapshot.showing {
                        let texture = texture.lock().unwrap().clone();
                        overlay::overlay(ui, &snapshot, texture.as_ref());
                    }
                });
            // Audio keeps arriving whether or not anything moves on screen.
            // Frame rate, not buffering, is what makes motion smooth here.
            //
            // The spectrogram advances well under a pixel between rebuilds, so
            // there is no content to double-buffer away — what was visible as
            // stepping was the overlay repainting fifteen times a second. Sixty
            // while anything moves, thirty at rest; a texture blit and a handful
            // of shapes is cheap enough that the difference does not show up in
            // the CPU figure.
            let interval = if snapshot.animating { 16 } else { 33 };
            ctx.request_repaint_after(Duration::from_millis(interval));
        });
    }

    /// Traced strokes as rectangles over the overlay's spectrogram, normalised
    /// to it.
    ///
    /// Computed here rather than in the overlay because the band on screen is
    /// whatever the zoom has settled on, and only this side knows it.
    fn overlay_strokes(&mut self) -> Vec<egui::Rect> {
        let cfg = self.app.config();
        if !cfg.overlay_spectrogram {
            return Vec::new();
        }
        let window = cfg.overlay_spectrogram_seconds.max(1.0);
        let now = self
            .snapshot
            .as_ref()
            .map(|s| s.timeline_seconds)
            .unwrap_or(0.0);
        let band = self.zoom.band(Instant::now());
        let Some(engine) = self.app.engine() else {
            return Vec::new();
        };
        let nyquist = engine.geometry().nyquist_hz();
        let scale = waterfall::FreqScale::new(band.low_hz, band.high_hz, nyquist);

        engine
            .traced_strokes()
            .iter()
            .filter_map(|stroke| {
                let ago_start = (now - stroke.start_seconds) as f32;
                let ago_end = (now - stroke.end_seconds) as f32;
                if ago_start > window {
                    return None;
                }
                // Time runs left to right, oldest at the left edge.
                let x0 = 1.0 - (ago_start / window).clamp(0.0, 1.0);
                let x1 = 1.0 - (ago_end.max(0.0) / window).clamp(0.0, 1.0);
                // `row` works in pixels, so ask it for a tall image and divide.
                const ROWS: usize = 1000;
                let y0 = scale.row(stroke.high_hz, ROWS) as f32 / ROWS as f32;
                let y1 = scale.row(stroke.low_hz, ROWS) as f32 / ROWS as f32;
                Some(egui::Rect::from_min_max(
                    egui::pos2(x0.min(x1), y0.min(y1)),
                    egui::pos2(x0.max(x1), y0.max(y1)),
                ))
            })
            .collect()
    }

    /// The lamp history, resampled onto the overlay spectrogram's time axis.
    ///
    /// One entry per slice, oldest first, so the overlay can draw it without
    /// knowing anything about timelines. Each slice takes the *strongest* rung
    /// seen inside it — a two-second detection must not vanish because the slice
    /// it fell in was mostly quiet.
    fn overlay_timeline(&self) -> Vec<Option<overlay::Rung>> {
        let cfg = self.app.config();
        if !cfg.overlay_spectrogram {
            return Vec::new();
        }
        self.timeline_slices(cfg.overlay_spectrogram_seconds, 480)
    }

    /// The lamp history resampled onto a time axis of `window_seconds`.
    ///
    /// Shared by the overlay strip and the main window's, so the two cannot
    /// disagree about what happened — they are the same measurement drawn at two
    /// widths, which is the point of putting it in both places.
    ///
    /// Each slice takes the *strongest* rung inside it. A two-second detection in
    /// a mostly-quiet slice is exactly what the strip exists to show, and an
    /// average or a last-value would erase it.
    fn timeline_slices(&self, window_seconds: f32, slices: usize) -> Vec<Option<overlay::Rung>> {
        if slices == 0 {
            return Vec::new();
        }
        let Some(now) = self.snapshot.as_ref().map(|s| s.timeline_seconds) else {
            return vec![None; slices];
        };

        self.timeline_slices_ending_at(now, window_seconds, slices)
    }

    fn timeline_slices_ending_at(
        &self,
        end_seconds: f64,
        window_seconds: f32,
        slices: usize,
    ) -> Vec<Option<overlay::Rung>> {
        if slices == 0 {
            return Vec::new();
        }
        let mut spans: Vec<(f64, f64, overlay::Rung)> = Vec::new();
        for record in self.app.events().iter().rev() {
            let e = &record.detection.event;
            spans.push((
                e.start_seconds,
                e.start_seconds + e.duration_seconds as f64,
                overlay::Rung::Anomaly,
            ));
        }
        if let Some(engine) = self.app.engine() {
            for stroke in engine.traced_strokes() {
                spans.push((
                    stroke.start_seconds,
                    stroke.end_seconds,
                    overlay::Rung::Signal,
                ));
            }
        }
        waterfall::project_spans(end_seconds, window_seconds.max(1.0) as f64, slices, &spans)
    }

    /// Rebuild the overlay's own spectrogram texture, at most as often as the
    /// analysis produces new rows.
    fn rebuild_overlay_spectrogram(&mut self, _ctx: &egui::Context) {
        let cfg = self.app.config();
        // Normally there is no point rebuilding faster than the analysis
        // produces rows. While the band is animating there is: the rows have not
        // changed, but where they are drawn has.
        let interval = if self.zoom.animating() {
            Duration::from_millis(16)
        } else {
            self.snapshot_interval
        };
        if !cfg.overlay_spectrogram || self.last_overlay_render.elapsed() < interval {
            return;
        }
        let Some(engine) = self.app.engine() else {
            return;
        };

        let geometry = engine.geometry();
        // The overlay's band is animated; the main window's is not. A cockpit
        // strip a few hundred pixels tall is where magnification actually buys
        // something, and the full view is where you go to see everything at once.
        let band = self.zoom.band(Instant::now());
        let scale = waterfall::FreqScale::new(band.low_hz, band.high_hz, geometry.nyquist_hz());
        let history = if cfg.spectrogram_show_excess {
            engine.excess_waterfall()
        } else {
            engine.waterfall()
        };
        let (w, h) = overlay_spectrogram_size(cfg, self.anchor.width, self.overlay_label_px);
        // Its own time window: a cockpit strip wants a short view, not the whole
        // analysis window crushed into a few hundred pixels.
        let window_frames = (cfg.overlay_spectrogram_seconds / geometry.frame_seconds())
            .ceil()
            .max(1.0) as usize;
        let image = waterfall::build_image(
            history,
            geometry,
            waterfall::RenderOptions {
                scale,
                auto_gain: true,
                median_subtract: cfg.spectrogram_median_subtract,
                window_frames,
                end_offset_frames: 0,
            },
            w as usize,
            h as usize,
        );
        // The timeline is painted into these pixels for the same reason as in
        // the main window: one buffer, formed together.
        let mut image = image;
        let slices = self.timeline_slices(cfg.overlay_spectrogram_seconds, w as usize);
        waterfall::paint_timeline(&mut image, &slices);

        // Handed over as pixels. The overlay viewport turns them into a texture
        // in its own pass; nothing here ever touches the GPU.
        self.pending_spectrogram = Some(image);
        self.last_overlay_render = Instant::now();
    }

    /// Write the current waterfall as a high-resolution PNG.
    ///
    /// The on-screen view is limited to the window; the published decodes were
    /// read at far higher resolution, which is the difference between seeing a
    /// mountain and seeing a smear.
    fn export_spectrogram(&mut self) -> Option<PathBuf> {
        let engine = self.app.engine()?;
        let geometry = engine.geometry();
        let cfg = self.app.config();
        let (min_hz, max_hz) = if self.waterfall_full_spectrum {
            (waterfall::DEFAULT_MIN_HZ, waterfall::FULL_SPECTRUM_MAX_HZ)
        } else {
            (cfg.spectrogram_min_hz, cfg.spectrogram_max_hz)
        };
        let scale = waterfall::FreqScale::new(min_hz, max_hz, geometry.nyquist_hz());
        let show_excess = cfg.spectrogram_show_excess;
        let history = if show_excess {
            engine.excess_waterfall()
        } else {
            engine.waterfall()
        };

        let dir = std::path::Path::new(&self.export_dir);
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::error!("could not create {}: {e}", dir.display());
            return None;
        }

        // Named by where it was taken, then when — so a folder of exports sorts
        // by system and the filename alone identifies the observation.
        let system = self
            .app
            .game_state()
            .star_system
            .unwrap_or_else(|| "unknown-system".to_string());
        let name = format!(
            "{}-{}{}.png",
            sanitize_for_filename(&system),
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
            if show_excess { "-excess" } else { "-raw" }
        );
        let path = dir.join(name);
        let mut written = None;
        let window_frames = (cfg.waterfall_seconds / geometry.frame_seconds())
            .ceil()
            .max(1.0) as usize;
        match waterfall::export_png(
            history,
            geometry,
            waterfall::RenderOptions {
                scale,
                auto_gain: true,
                median_subtract: cfg.spectrogram_median_subtract,
                window_frames,
                end_offset_frames: 0,
            },
            cfg.export_width,
            export_height(cfg),
            &path,
        ) {
            Ok(()) => {
                log::info!("exported {}", path.display());
                written = Some(path.clone());
                // Exports are renderings of data still held elsewhere, so they
                // are trimmed oldest-first with no ranking. Without this they
                // are the one thing on disk with no ceiling at all.
                crate::retention::enforce_simple_budget(
                    dir,
                    "png",
                    cfg.export_budget_mb.saturating_mul(1_048_576),
                );
            }
            Err(e) => log::error!("could not export the spectrogram: {e}"),
        }
        written
    }

    /// Write the audio *and* the picture, on one press.
    ///
    /// These were two buttons in two different rows, which is one too many for
    /// the moment they exist for: something is on screen that should not be, and
    /// whatever is kept now is all anyone will ever have of it. Sound and image
    /// answer different questions — the image shows what the detectors saw, the
    /// audio can be re-run through them with different thresholds — and needing
    /// both is the normal case, not the careful one.
    fn export_everything(&mut self) {
        // As much as the ring holds, not an arbitrary minute. The Landscape
        // Signal's cycle alone is 109.5 s, and a dump too short to contain one
        // cannot answer anything about it afterwards.
        let seconds = self.app.config().pcm_ring_seconds;
        let mut why = String::new();
        let audio: Option<PathBuf> = match self.app.keep_recent(seconds, "manual", true) {
            Ok(path) => Some(path),
            Err(e) => {
                log::warn!("could not keep audio: {e:#}");
                why = format!(" ({e})");
                None
            }
        };
        let png = self.export_spectrogram();
        let message = match (audio, png) {
            (Some(a), Some(_)) => format!(
                "exported {:.0} s of audio and the spectrogram to {}",
                seconds,
                a.parent()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default()
            ),
            (Some(a), None) => format!("kept the audio ({}), but the image failed", a.display()),
            (None, Some(p)) => format!("wrote {}, but the audio failed{why}", p.display()),
            (None, None) => format!("export failed{why} — see the log"),
        };
        self.export_status = Some((message, Instant::now()));
    }

    fn detectors(&mut self, ui: &mut egui::Ui) {
        let Some(snap) = &self.snapshot else { return };
        let cfg = self.app.config();
        let good = egui::Color32::from_rgb(120, 255, 160);
        let idle = egui::Color32::from_gray(120);

        ui.horizontal(|ui| {
            // Binary keying.
            match &snap.keying {
                Some(k) if k.is_present(cfg.keying_threshold) => {
                    ui.label(
                        egui::RichText::new("◉ TRANSMISSION")
                            .monospace()
                            .size(16.0)
                            .color(good),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{:.2} · {} tones · {:.2} sym/s",
                            k.confidence,
                            k.tones_hz.len(),
                            k.symbol_rate_hz
                        ))
                        .monospace(),
                    )
                    .on_hover_text(format!(
                        "tones: {:?} Hz\ntiming regularity {:.2}, alphabet purity {:.2}",
                        k.tones_hz
                            .iter()
                            .map(|h| h.round() as i32)
                            .collect::<Vec<_>>(),
                        k.timing_regularity,
                        k.alphabet_purity
                    ));
                }
                Some(k) => {
                    ui.label(
                        egui::RichText::new(format!("○ no keying  {:.2}", k.confidence))
                            .monospace()
                            .color(idle),
                    );
                }
                None => {
                    ui.label(egui::RichText::new("○ no keying").monospace().color(idle));
                }
            }

            ui.separator();

            // Drawn structure.
            let st = &snap.structure;
            if st.is_present(cfg.structure_threshold) {
                ui.label(
                    egui::RichText::new("◉ PICTURE")
                        .monospace()
                        .size(16.0)
                        .color(good),
                );
                ui.label(egui::RichText::new(format!("{:.2}", st.score)).monospace())
                    .on_hover_text(format!(
                        "coherence {:.2}, sparsity {:.2}, orientation diversity {:.2}",
                        st.coherence, st.sparsity, st.orientation_diversity
                    ));
            } else {
                ui.label(
                    egui::RichText::new(format!("○ no picture  {:.2}", st.score))
                        .monospace()
                        .color(idle),
                );
            }
        });
    }

    fn instruments(&mut self, ui: &mut egui::Ui) {
        let snapshot = self.snapshot.clone();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("AZIMUTH").monospace().size(11.0));
                match &snapshot {
                    Some(s) => compass::draw(ui, &s.direction, 140.0),
                    None => {
                        ui.allocate_space(egui::vec2(140.0, 140.0));
                    }
                }
                if snapshot
                    .as_ref()
                    .is_some_and(|s| s.direction.front_back_ambiguous && s.direction.is_usable())
                {
                    ui.weak(
                        egui::RichText::new("front/back ambiguous")
                            .monospace()
                            .size(10.0),
                    )
                    .on_hover_text(
                        "Two channels cannot distinguish a source ahead from one astern. \
                         Switch the Windows output endpoint to 7.1 to resolve it.",
                    );
                }
            });

            ui.add_space(16.0);

            ui.vertical(|ui| {
                ui.label(egui::RichText::new("PERIODICITY").monospace().size(11.0));
                compass::draw_periodicity(
                    ui,
                    snapshot.as_ref().and_then(|s| s.periodicity.as_ref()),
                    egui::vec2(320.0, 120.0),
                    30.0,
                    600.0,
                );
                if let Some(p) = snapshot.as_ref().and_then(|s| s.periodicity.as_ref())
                    && crate::analysis::periodicity::matches_landscape(p, 2.0)
                {
                    ui.label(
                        egui::RichText::new("consistent with the Landscape Signal")
                            .monospace()
                            .size(11.0)
                            .color(egui::Color32::from_rgb(120, 200, 255)),
                    );
                }
            });

            ui.add_space(16.0);

            ui.vertical(|ui| {
                ui.label(egui::RichText::new("CHANNELS").monospace().size(11.0));
                if let Some(s) = &snapshot {
                    controls::channel_meters(ui, s, egui::vec2(220.0, 120.0));
                }
                controls::muted_bands(ui, &mut self.app);
            });
        });
    }
}

impl eframe::App for CompassUi {
    /// Paint this window solid.
    ///
    /// The root viewport asks for transparency so the GL config has an alpha
    /// channel — without it the overlay, a child viewport, cannot be
    /// transparent and appears as a black rectangle over the cockpit. That
    /// request would otherwise let the desktop show through *this* window too,
    /// so the clear colour is pinned opaque here.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        let [r, g, b, _] = visuals.window_fill().to_normalized_gamma_f32();
        [r, g, b, 1.0]
    }

    /// Runs before every repaint, and also when the window is hidden — which is
    /// exactly where draining capture belongs, so a minimized window does not
    /// stall the analysis.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // A gap here is the one thing that can destroy the overlay, so it is
        // worth saying so out loud rather than discovering it in flight again.
        let since = self.last_logic.elapsed();
        if cfg!(windows) && since >= Duration::from_millis(500) {
            log::warn!(
                "no frame for {:.1} s — the overlay viewport may have lapsed",
                since.as_secs_f32()
            );
        }
        self.last_logic = Instant::now();

        if !self.app.config().setup_complete {
            ctx.request_repaint_after(Duration::from_millis(250));
            return;
        }

        self.app.pump();
        if self.last_snapshot.elapsed() >= self.snapshot_interval {
            self.snapshot = self.app.snapshot();
            self.last_snapshot = Instant::now();
            if let Some(snapshot) = &self.snapshot {
                self.waterfall_view.update(snapshot.timeline_seconds);
            }
            let cfg = self.app.config();
            let active = cfg
                .overlay_zoom_on_detection
                .then(|| {
                    self.snapshot
                        .as_ref()
                        .and_then(|s| s.active_band_hz)
                        .map(|(low, high)| zoom::Band::new(low, high))
                })
                .flatten();
            self.zoom.set_bounds(zoom::Band::new(
                cfg.spectrogram_min_hz,
                cfg.spectrogram_max_hz,
            ));
            self.zoom.observe(active, Instant::now());
        }
        // The overlay is driven from here, NOT from `ui`, for the same reason
        // `pump` is: `ui` stops running the moment the main window is
        // minimized — which is exactly how the tool is used while flying — and
        // an overlay fed from `ui` freezes at whatever state it was last
        // handed. The game's focus comes from our own Win32 poll rather than
        // from egui input, so visibility stays correct even then.
        self.sync_overlay(ctx);
        // Audio keeps arriving whether or not anything moves on screen, so the
        // window must repaint without waiting for input — and faster than that
        // while the overlay's band is moving, since a 450 ms animation redrawn
        // ten times a second is a slideshow rather than a movement.
        let interval = if cfg!(target_os = "macos") {
            // No cockpit overlay or animated overlay zoom exists on macOS. The
            // analysis still runs at full rate; this only caps GUI repaints.
            66
        } else if self.zoom.animating() {
            16
        } else {
            33
        };
        ctx.request_repaint_after(Duration::from_millis(interval));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.app.config().setup_complete {
            let ctx = ui.ctx().clone();
            self.first_launch_setup(&ctx, ui);
            return;
        }
        egui::Panel::top("header").show(ui, |ui| {
            ui.add_space(4.0);
            self.header(ui);
            ui.add_space(2.0);
            self.detectors(ui);
            ui.add_space(2.0);
            self.controls_row(ui);
            ui.add_space(4.0);
        });

        egui::Panel::bottom("events")
            .resizable(true)
            .default_size(180.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new("EVENTS").monospace().size(11.0));
                events::draw(ui, self.app.events());
            });

        egui::Panel::bottom("health").show(ui, |ui| {
            if let Some(s) = &self.snapshot {
                controls::health_strip(ui, s);
            }
            overlay::disk_usage(ui, &mut self.app);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            self.waterfall_overview(ui);
            ui.add_space(4.0);
            let analysis_viewport_height = ui.available_height();
            egui::ScrollArea::vertical()
                .id_salt("analysis-body")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.waterfall_panels(ui, analysis_viewport_height);
                    ui.add_space(6.0);
                    self.instruments(ui);
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_overlay_follows_the_game_s_focus() {
        assert!(overlay_visible(true, true), "Elite in front: show");
        assert!(
            !overlay_visible(true, false),
            "Alt-Tabbed away: the overlay must go with it, not float over the browser"
        );
    }

    #[test]
    fn it_starts_hidden_and_is_revealed_only_by_the_game() {
        // At launch our own control window has focus and Elite does not. An
        // earlier version treated our focus as reason enough to show, so the
        // overlay appeared for a moment on every start and then disappeared.
        assert!(
            !overlay_visible(true, false),
            "our window focused, Elite not: stay hidden"
        );
        assert!(
            !overlay_visible(false, false),
            "no game at all: stay hidden"
        );
        // Only the game brings it out.
        assert!(overlay_visible(true, true));
    }

    #[test]
    fn no_game_window_means_no_overlay_at_all() {
        // Regression: the overlay used to appear over the bare desktop whenever
        // the control panel had focus, flashing as it fought itself for focus.
        assert!(!overlay_visible(false, false));
        assert!(
            !overlay_visible(false, true),
            "a focused window we cannot find is not the game"
        );
    }

    #[test]
    fn visibility_is_stable_while_the_game_stays_focused() {
        // Flapping here is flicker on screen. Focus held steady must give a
        // steady answer, whatever else changes around it.
        for _ in 0..10 {
            assert!(overlay_visible(true, true));
        }
    }

    #[test]
    fn the_overlay_state_is_shareable_with_a_viewport_callback() {
        // `show_viewport_deferred` demands `Fn + Send + Sync + 'static`, and
        // the state must be readable from it long after the frame that wrote
        // it. If this stops compiling, the overlay is about to go stale again.
        fn assert_shareable<T: Send + Sync + 'static>() {}
        assert_shareable::<Arc<Mutex<overlay::OverlayState>>>();

        let shared = Arc::new(Mutex::new(overlay::OverlayState::default()));
        let reader = Arc::clone(&shared);
        shared.lock().unwrap().rung = Some(overlay::Rung::Signal);
        // A later write is visible to the holder of the clone — which is the
        // whole reason the callback reads through one.
        assert_eq!(reader.lock().unwrap().rung, Some(overlay::Rung::Signal));
    }

    #[test]
    fn the_overlay_window_is_never_torn_down_to_hide_it() {
        // Both of the obvious hide mechanisms are broken in egui 0.35, each in
        // a way that took a crash or a freeze to find, so the visibility
        // decision must only ever change what is *painted*.
        //
        // Destroying the viewport risks `Painter::set_window(id, None)`, which
        // clears every surface, not just that viewport's. Hiding it with
        // `with_visible(false)` stops its redraws for good. Painting nothing
        // has neither failure mode.
        // Nothing is painted until the visibility decision has actually been
        // made, so launching cannot flash a panel over the desktop.
        let mut state = overlay::OverlayState::default();
        assert!(!state.showing, "an overlay paints nothing until told to");
        state.showing = true;
        assert!(state.showing, "and the flag is what the callback reads");
    }

    #[test]
    fn the_overlay_is_not_the_root_viewport() {
        // ViewportId(Id::NULL) is ROOT — the control window. Reusing it would
        // have made the overlay replace the window it is meant to accompany.
        assert_ne!(overlay_viewport_id(), egui::ViewportId::ROOT);
    }

    #[test]
    fn time_zoom_preserves_the_inspected_center() {
        let mut view = TimeViewport::new(140.0);
        view.set_duration(200.0, 70.0);
        view.inspect_age(200.0, 80.0);
        let old_center = view.inspected_end_seconds.unwrap() - 35.0;
        view.set_duration(210.0, 35.0);
        let new_center = view.inspected_end_seconds.unwrap() - 17.5;
        assert!((old_center - new_center).abs() < 0.001);
    }

    #[test]
    fn live_zoom_stays_attached_to_now() {
        let mut view = TimeViewport::new(140.0);
        view.set_duration(200.0, 35.0);
        assert!(view.is_live());
        assert_eq!(view.end_offset(500.0), 0.0);
        assert_eq!(view.overview_range(500.0), (0.75, 1.0));
    }

    #[test]
    fn inspected_history_moves_left_and_eventually_expires() {
        let mut view = TimeViewport::new(140.0);
        view.set_duration(200.0, 35.0);
        view.inspect_age(200.0, 70.0);
        let first = view.overview_range(200.0);
        let later = view.overview_range(220.0);
        assert!(later.0 < first.0 && later.1 < first.1);
        view.update(400.0);
        assert!(view.is_live());
    }

    #[test]
    fn the_full_view_has_no_historical_navigation() {
        let mut view = TimeViewport::new(140.0);
        view.inspect_age(200.0, 70.0);
        assert!(view.is_live());
        assert_eq!(view.overview_range(200.0), (0.0, 1.0));
    }

    #[test]
    fn short_mac_views_refresh_faster_than_the_full_timeline() {
        let snapshot = Duration::from_millis(100);
        let full = main_waterfall_interval(140.0, 1000.0, snapshot, true);
        let close = main_waterfall_interval(15.0, 1000.0, snapshot, true);
        assert_eq!(full, Duration::from_millis(250));
        assert!(close < Duration::from_millis(70));
        assert_eq!(
            main_waterfall_interval(15.0, 1000.0, snapshot, false),
            snapshot
        );
    }

    #[test]
    fn dragging_preserves_the_grabbed_point_in_the_viewport() {
        let box_width = 0.25;
        assert_eq!(dragged_view_center(0.4, 0.5, box_width), 0.4);
        assert_eq!(dragged_view_center(0.4, 0.0, box_width), 0.525);
        assert_eq!(dragged_view_center(0.4, 1.0, box_width), 0.275);
        assert_eq!(dragged_view_center(0.0, 1.0, box_width), 0.0);
        assert_eq!(dragged_view_center(1.0, 0.0, box_width), 1.0);
    }
}
