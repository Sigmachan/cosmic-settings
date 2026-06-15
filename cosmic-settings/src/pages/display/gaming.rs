// Copyright 2026 Kira Keller <senedato@gmail.com>
// SPDX-License-Identifier: GPL-3.0-only

//! Gaming settings — sub-page of the Display page.
//!
//! Configures default Gamescope launch options. The settings are written to
//! `~/.config/cosmic-gamescope.conf`, which the `gamescope-launch` wrapper reads
//! to build the Gamescope command line. Use it in Steam by setting a game's
//! launch options to `gamescope-launch %command%`.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, column, list_column, row, settings, text, toggler};
use cosmic::{Apply, Element, Task};
use cosmic_settings_page::{self as page, Section, section};

/// Best-effort live control of a *running* wayscope/gamescope session via the
/// `wayscope-dbus` daemon (DBus name `org.shadowblip.Gamescope`). When no
/// daemon/session is present the call simply errors and is ignored — the conf
/// file remains the source of truth for the next launch.
mod wayscope_dbus {
    #[zbus::proxy(
        interface = "org.shadowblip.Gamescope.XWayland.Primary",
        default_service = "org.shadowblip.Gamescope",
        default_path = "/org/shadowblip/Gamescope/XWayland0"
    )]
    pub trait Primary {
        fn set_hdr_enabled(&self, enable: bool) -> zbus::Result<()>;
        fn set_vrr_enabled(&self, enable: bool) -> zbus::Result<()>;
        fn set_fps_limit(&self, fps: u32) -> zbus::Result<()>;
    }

    /// A setting that can be applied to the live session immediately.
    #[derive(Debug, Clone)]
    pub enum Live {
        Hdr(bool),
        Vrr(bool),
        Fps(u32),
    }

    pub async fn apply(change: Live) -> Result<(), String> {
        let conn = zbus::Connection::session().await.map_err(|e| e.to_string())?;
        let proxy = PrimaryProxy::new(&conn).await.map_err(|e| e.to_string())?;
        match change {
            Live::Hdr(v) => proxy.set_hdr_enabled(v).await,
            Live::Vrr(v) => proxy.set_vrr_enabled(v).await,
            Live::Fps(v) => proxy.set_fps_limit(v).await,
        }
        .map_err(|e| e.to_string())
    }
}

/// Fire a best-effort live-apply at the running session (no-op if absent).
fn live_task(change: wayscope_dbus::Live) -> Task<crate::app::Message> {
    cosmic::task::future(async move {
        crate::app::Message::from(Message::LiveApplied(wayscope_dbus::apply(change).await))
    })
}

// ── config ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct GamingConf {
    pub out_width: u32,
    pub out_height: u32,
    /// Render scale as a percentage of the output resolution (50–100). 100 = native.
    pub render_scale: u32,
    pub upscaler: String,
    pub sharpness: u32,
    pub fps_limit: u32,
    pub hdr: bool,
    pub adaptive_sync: bool,
    pub mangoapp: bool,
    pub force_grab_cursor: bool,
    pub steam: bool,
    pub prefer_discrete: bool,
    // NVIDIA-only gaming features (applied as env by gamescope-launch)
    pub nv_smooth_motion: bool,
    pub nv_reflex: bool,
    pub nv_vibrance: i32,
    pub nv_dldsr: bool,
}

impl Default for GamingConf {
    fn default() -> Self {
        Self {
            out_width: 3840,
            out_height: 2160,
            render_scale: 100,
            upscaler: "auto".into(),
            sharpness: 2,
            fps_limit: 0,
            hdr: false,
            adaptive_sync: true,
            mangoapp: false,
            force_grab_cursor: false,
            steam: false,
            prefer_discrete: true,
            nv_smooth_motion: false,
            nv_reflex: true,
            nv_vibrance: 0,
            nv_dldsr: false,
        }
    }
}

fn conf_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    base.join("cosmic-gamescope.conf")
}

fn read_conf() -> GamingConf {
    let mut c = GamingConf::default();
    let Ok(s) = std::fs::read_to_string(conf_path()) else {
        return c;
    };
    for line in s.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match k.trim() {
            "OUT_WIDTH" => c.out_width = v.parse().unwrap_or(c.out_width),
            "OUT_HEIGHT" => c.out_height = v.parse().unwrap_or(c.out_height),
            "RENDER_SCALE" => c.render_scale = v.parse::<u32>().unwrap_or(c.render_scale).clamp(50, 100),
            "UPSCALER" => {
                if UPSCALERS.contains(&v) {
                    c.upscaler = v.to_owned();
                }
            }
            "SHARPNESS" => c.sharpness = v.parse::<u32>().unwrap_or(c.sharpness).min(20),
            "FPS_LIMIT" => c.fps_limit = v.parse().unwrap_or(c.fps_limit),
            "HDR" => c.hdr = v == "1",
            "ADAPTIVE_SYNC" => c.adaptive_sync = v == "1",
            "MANGOAPP" => c.mangoapp = v == "1",
            "FORCE_GRAB_CURSOR" => c.force_grab_cursor = v == "1",
            "STEAM" => c.steam = v == "1",
            "PREFER_DISCRETE" => c.prefer_discrete = v == "1",
            "NV_SMOOTH_MOTION" => c.nv_smooth_motion = v == "1",
            "NV_REFLEX" => c.nv_reflex = v == "1",
            "NV_VIBRANCE" => c.nv_vibrance = v.parse().unwrap_or(c.nv_vibrance),
            "NV_DLDSR" => c.nv_dldsr = v == "1",
            _ => {}
        }
    }
    c
}

impl GamingConf {
    fn to_conf_string(&self) -> String {
        format!(
            "OUT_WIDTH={}\nOUT_HEIGHT={}\nRENDER_SCALE={}\nUPSCALER={}\nSHARPNESS={}\n\
             FPS_LIMIT={}\nHDR={}\nADAPTIVE_SYNC={}\nMANGOAPP={}\nFORCE_GRAB_CURSOR={}\n\
             STEAM={}\nPREFER_DISCRETE={}\n\
             NV_SMOOTH_MOTION={}\nNV_REFLEX={}\nNV_VIBRANCE={}\nNV_DLDSR={}\n",
            self.out_width,
            self.out_height,
            self.render_scale,
            self.upscaler,
            self.sharpness,
            self.fps_limit,
            self.hdr as u8,
            self.adaptive_sync as u8,
            self.mangoapp as u8,
            self.force_grab_cursor as u8,
            self.steam as u8,
            self.prefer_discrete as u8,
            self.nv_smooth_motion as u8,
            self.nv_reflex as u8,
            self.nv_vibrance,
            self.nv_dldsr as u8,
        )
    }

    /// Resolved Gamescope command-line preview (what `gamescope-launch` runs).
    fn command_preview(&self) -> String {
        let mut a = vec!["gamescope".to_string()];
        a.push(format!("-W {} -H {}", self.out_width, self.out_height));
        if self.render_scale < 100 {
            let rw = self.out_width * self.render_scale / 100;
            let rh = self.out_height * self.render_scale / 100;
            a.push(format!("-w {rw} -h {rh}"));
        }
        if self.upscaler != "auto" {
            a.push(format!("-F {}", self.upscaler));
            if matches!(self.upscaler.as_str(), "fsr" | "nis") {
                a.push(format!("--sharpness {}", self.sharpness));
            }
        }
        if self.fps_limit > 0 {
            a.push(format!("-r {}", self.fps_limit));
        }
        if self.hdr {
            a.push("--hdr-enabled".into());
        }
        if self.adaptive_sync {
            a.push("--adaptive-sync".into());
        }
        if self.mangoapp {
            a.push("--mangoapp".into());
        }
        if self.force_grab_cursor {
            a.push("--force-grab-cursor".into());
        }
        if self.steam {
            a.push("-e".into());
        }
        a.push("-- %command%".into());
        a.join(" ")
    }
}

async fn write_conf(c: GamingConf) -> Result<(), String> {
    let path = conf_path();
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&path, c.to_conf_string())
        .await
        .map_err(|e| e.to_string())
}

// ── option tables ─────────────────────────────────────────────────────────────

const GS_RESOLUTIONS: &[(u32, u32, &str)] = &[
    (1920, 1080, "1920 × 1080  (1080p)"),
    (2560, 1440, "2560 × 1440  (1440p)"),
    (3440, 1440, "3440 × 1440  (UW 1440p)"),
    (3840, 2160, "3840 × 2160  (4K UHD)"),
    (5120, 2880, "5120 × 2880  (5K)"),
    (7680, 4320, "7680 × 4320  (8K)"),
];

const UPSCALERS: &[&str] = &["auto", "fsr", "nis", "integer", "stretch", "linear", "nearest"];
const UPSCALER_LABELS: &[&str] = &[
    "Auto",
    "AMD FSR 1.0",
    "NVIDIA Image Scaling",
    "Integer scale",
    "Stretch",
    "Linear",
    "Nearest-neighbour",
];

// ── messages ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(GamingConf, &'static str),
    Resolution(u32, u32),
    RenderScale(u32),
    Upscaler(usize),
    Sharpness(u32),
    FpsLimit(u32),
    Hdr(bool),
    AdaptiveSync(bool),
    Mangoapp(bool),
    ForceGrabCursor(bool),
    Steam(bool),
    PreferDiscrete(bool),
    NvSmoothMotion(bool),
    NvReflex(bool),
    NvVibrance(i32),
    NvDldsr(bool),
    Save,
    Saved(Result<(), String>),
    /// Result of a best-effort live apply to a running session.
    LiveApplied(Result<(), String>),
}

impl From<Message> for crate::pages::Message {
    fn from(m: Message) -> Self {
        crate::pages::Message::DisplayGaming(m)
    }
}

impl From<Message> for crate::Message {
    fn from(m: Message) -> Self {
        crate::Message::PageMessage(m.into())
    }
}

fn gpu_vendor() -> &'static str {
    if std::path::Path::new("/dev/nvidia0").exists() {
        return "NVIDIA";
    }
    for entry in std::fs::read_dir("/sys/class/drm")
        .into_iter()
        .flatten()
        .flatten()
    {
        if let Ok(v) = std::fs::read_to_string(entry.path().join("device/vendor")) {
            match v.trim() {
                "0x1002" => return "AMD",
                "0x8086" => return "Intel",
                _ => {}
            }
        }
    }
    "unknown"
}

fn gamescope_installed() -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|p| {
                p.join("gamescope").exists() || p.join("gamescope-launch").exists()
            })
        })
        .unwrap_or(false)
}

// ── page state ────────────────────────────────────────────────────────────────

pub struct Page {
    entity: page::Entity,
    conf: GamingConf,
    gpu: &'static str,
    status: Option<String>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            entity: page::Entity::default(),
            conf: GamingConf::default(),
            gpu: "unknown",
            status: None,
        }
    }
}

impl Page {
    pub fn update(&mut self, message: Message) -> Task<crate::app::Message> {
        match message {
            Message::Loaded(conf, gpu) => {
                self.conf = conf;
                self.gpu = gpu;
            }
            Message::Resolution(w, h) => {
                self.conf.out_width = w;
                self.conf.out_height = h;
            }
            Message::RenderScale(v) => self.conf.render_scale = v,
            Message::Upscaler(i) => {
                self.conf.upscaler = UPSCALERS[i.min(UPSCALERS.len() - 1)].into();
            }
            Message::Sharpness(v) => self.conf.sharpness = v,
            Message::FpsLimit(v) => {
                self.conf.fps_limit = v;
                return live_task(wayscope_dbus::Live::Fps(v));
            }
            Message::Hdr(v) => {
                self.conf.hdr = v;
                return live_task(wayscope_dbus::Live::Hdr(v));
            }
            Message::AdaptiveSync(v) => {
                self.conf.adaptive_sync = v;
                return live_task(wayscope_dbus::Live::Vrr(v));
            }
            Message::Mangoapp(v) => self.conf.mangoapp = v,
            Message::ForceGrabCursor(v) => self.conf.force_grab_cursor = v,
            Message::Steam(v) => self.conf.steam = v,
            Message::PreferDiscrete(v) => self.conf.prefer_discrete = v,
            Message::NvSmoothMotion(v) => self.conf.nv_smooth_motion = v,
            Message::NvReflex(v) => self.conf.nv_reflex = v,
            Message::NvVibrance(v) => self.conf.nv_vibrance = v,
            Message::NvDldsr(v) => self.conf.nv_dldsr = v,
            Message::Save => {
                self.status = Some("Saving…".into());
                let c = self.conf.clone();
                return cosmic::task::future(async move {
                    crate::app::Message::from(Message::Saved(write_conf(c).await))
                });
            }
            Message::Saved(Ok(())) => self.status = Some("Saved ✓".into()),
            Message::Saved(Err(e)) => self.status = Some(format!("Error: {e}")),
            // Live apply is best-effort: success shows a subtle hint, absence of
            // a running wayscope session errors silently (conf still persists).
            Message::LiveApplied(Ok(())) => self.status = Some("Applied to running session ✓".into()),
            Message::LiveApplied(Err(_)) => {}
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, crate::pages::Message> {
        let theme = cosmic::theme::active();
        let sp = &theme.cosmic().spacing;
        let mut col = column::with_capacity(16).spacing(sp.space_m);

        // ── intro / launch usage ──────────────────────────────────────────────
        col = col.push(
            list_column().add(
                settings::item::builder("Gamescope launch wrapper")
                    .description(
                        "Set a Steam game's launch options to:  gamescope-launch %command%\n\
                         These defaults are written to ~/.config/cosmic-gamescope.conf",
                    )
                    .control(text::caption(if gamescope_installed() {
                        format!("{} GPU · gamescope detected ✓", self.gpu)
                    } else {
                        format!("{} GPU · gamescope not found in PATH", self.gpu)
                    })),
            ),
        );

        // ── output + scaling ──────────────────────────────────────────────────
        col = col.push(text::heading("Output & Scaling"));

        let res_opts: Vec<String> = GS_RESOLUTIONS.iter().map(|&(_, _, l)| l.to_string()).collect();
        let res_sel = GS_RESOLUTIONS
            .iter()
            .position(|&(w, h, _)| w == self.conf.out_width && h == self.conf.out_height);

        let upscaler_opts: Vec<String> = UPSCALER_LABELS.iter().map(|s| s.to_string()).collect();
        let upscaler_sel = UPSCALERS.iter().position(|&m| m == self.conf.upscaler);

        let render_label = if self.conf.render_scale >= 100 {
            "Native".to_string()
        } else {
            let rw = self.conf.out_width * self.conf.render_scale / 100;
            let rh = self.conf.out_height * self.conf.render_scale / 100;
            format!("{}%  ({rw}×{rh})", self.conf.render_scale)
        };

        col = col.push(
            list_column()
                .add(
                    settings::item::builder("Output Resolution")
                        .description("Resolution Gamescope presents to the display")
                        .control(
                            widget::dropdown(res_opts, res_sel, |i| {
                                let (w, h, _) = GS_RESOLUTIONS[i.min(GS_RESOLUTIONS.len() - 1)];
                                msg(Message::Resolution(w, h))
                            })
                            .width(Length::Fixed(260.0)),
                        ),
                )
                .add(
                    settings::item::builder("Render Scale")
                        .description("Render below output resolution, then upscale (FSR/NIS)")
                        .control(
                            row::with_capacity(2)
                                .spacing(sp.space_s)
                                .align_y(Alignment::Center)
                                .push(
                                    widget::slider(50..=100u32, self.conf.render_scale, |v| {
                                        msg(Message::RenderScale(v))
                                    })
                                    .step(5u32)
                                    .width(Length::Fill),
                                )
                                .push(
                                    text::body(render_label)
                                        .apply(widget::container)
                                        .width(Length::Fixed(140.0)),
                                ),
                        ),
                )
                .add(
                    settings::item::builder("Upscaler")
                        .description("Algorithm used when rendering below output resolution")
                        .control(
                            widget::dropdown(upscaler_opts, upscaler_sel, |i| {
                                msg(Message::Upscaler(i))
                            })
                            .width(Length::Fixed(260.0)),
                        ),
                )
                .add(
                    settings::item::builder("Sharpness")
                        .description("FSR / NIS sharpening strength (0 = off, 20 = max)")
                        .control(
                            row::with_capacity(2)
                                .spacing(sp.space_s)
                                .align_y(Alignment::Center)
                                .push(
                                    widget::slider(0..=20u32, self.conf.sharpness, |v| {
                                        msg(Message::Sharpness(v))
                                    })
                                    .width(Length::Fill),
                                )
                                .push(
                                    text::body(format!("{}", self.conf.sharpness))
                                        .apply(widget::container)
                                        .width(Length::Fixed(48.0)),
                                ),
                        ),
                ),
        );

        // ── frame pacing ──────────────────────────────────────────────────────
        col = col.push(text::heading("Frame Pacing"));
        let fps_label = if self.conf.fps_limit == 0 {
            "Off".to_string()
        } else {
            format!("{} fps", self.conf.fps_limit)
        };
        col = col.push(
            list_column()
                .add(
                    settings::item::builder("Frame Rate Limit")
                        .description("Cap the in-game frame rate (0 = unlimited)")
                        .control(
                            row::with_capacity(2)
                                .spacing(sp.space_s)
                                .align_y(Alignment::Center)
                                .push(
                                    widget::slider(0..=360u32, self.conf.fps_limit, |v| {
                                        msg(Message::FpsLimit(v))
                                    })
                                    .step(5u32)
                                    .width(Length::Fill),
                                )
                                .push(
                                    text::body(fps_label)
                                        .apply(widget::container)
                                        .width(Length::Fixed(72.0)),
                                ),
                        ),
                )
                .add(
                    settings::item::builder("Adaptive Sync (VRR)")
                        .description("Variable refresh rate when the display and driver support it")
                        .control(
                            toggler(self.conf.adaptive_sync)
                                .on_toggle(|v| msg(Message::AdaptiveSync(v))),
                        ),
                ),
        );

        // ── output features ───────────────────────────────────────────────────
        col = col.push(text::heading("Features"));
        col = col.push(
            list_column()
                .add(
                    settings::item::builder("HDR")
                        .description("Enable Gamescope HDR output for the game (--hdr-enabled)")
                        .control(toggler(self.conf.hdr).on_toggle(|v| msg(Message::Hdr(v)))),
                )
                .add(
                    settings::item::builder("MangoHud overlay")
                        .description("Show the mangoapp performance overlay")
                        .control(
                            toggler(self.conf.mangoapp).on_toggle(|v| msg(Message::Mangoapp(v))),
                        ),
                )
                .add(
                    settings::item::builder("Force grab cursor")
                        .description("Confine and grab the cursor — fixes mouse in some games")
                        .control(
                            toggler(self.conf.force_grab_cursor)
                                .on_toggle(|v| msg(Message::ForceGrabCursor(v))),
                        ),
                )
                .add(
                    settings::item::builder("Steam integration")
                        .description("Embedded Steam mode (-e) for Steam input and overlay")
                        .control(toggler(self.conf.steam).on_toggle(|v| msg(Message::Steam(v)))),
                )
                .add(
                    settings::item::builder("Prefer discrete GPU")
                        .description("Render on the discrete GPU on hybrid systems")
                        .control(
                            toggler(self.conf.prefer_discrete)
                                .on_toggle(|v| msg(Message::PreferDiscrete(v))),
                        ),
                ),
        );

        // ── NVIDIA gaming (only on NVIDIA) ────────────────────────────────────
        if self.gpu == "NVIDIA" {
            col = col.push(text::heading("NVIDIA"));
            let vibrance_row = settings::item::builder("Digital Vibrance")
                .description("nvibrant — 0 = neutral, 1023 = maximum saturation")
                .control(
                    row::with_capacity(2)
                        .spacing(sp.space_s)
                        .align_y(Alignment::Center)
                        .push(
                            widget::slider(-1024..=1023i32, self.conf.nv_vibrance, |v| {
                                msg(Message::NvVibrance(v))
                            })
                            .width(Length::Fill),
                        )
                        .push(
                            text::body(format!("{}", self.conf.nv_vibrance))
                                .apply(widget::container)
                                .width(Length::Fixed(52.0)),
                        ),
                );
            col = col.push(
                list_column()
                    .add(
                        settings::item::builder("RTX Smooth Motion")
                            .description("Driver frame generation (NVPRESENT_ENABLE_SMOOTH_MOTION, driver 575+)")
                            .control(
                                toggler(self.conf.nv_smooth_motion)
                                    .on_toggle(|v| msg(Message::NvSmoothMotion(v))),
                            ),
                    )
                    .add(
                        settings::item::builder("NVIDIA Reflex (Low Latency)")
                            .description("PROTON_ENABLE_NVAPI + DXVK_ENABLE_NVAPI — Proton/DXVK games")
                            .control(
                                toggler(self.conf.nv_reflex).on_toggle(|v| msg(Message::NvReflex(v))),
                            ),
                    )
                    .add(
                        settings::item::builder("DLDSR 2.25×")
                            .description("Deep-learning dynamic super-resolution downscale")
                            .control(
                                toggler(self.conf.nv_dldsr).on_toggle(|v| msg(Message::NvDldsr(v))),
                            ),
                    )
                    .add(vibrance_row),
            );
        }

        // ── command preview ───────────────────────────────────────────────────
        col = col.push(text::heading("Command Preview"));
        col = col.push(
            text::body(self.conf.command_preview())
                .apply(widget::container)
                .padding([sp.space_xs, sp.space_s])
                .width(Length::Fill),
        );

        // ── action row ────────────────────────────────────────────────────────
        let mut action_row = row::with_capacity(2)
            .spacing(sp.space_s)
            .align_y(Alignment::Center);
        if let Some(ref s) = self.status {
            action_row = action_row
                .push(text::caption(s.as_str()).apply(widget::container).width(Length::Fill));
        } else {
            action_row = action_row.push(widget::Space::new().width(Length::Fill));
        }
        action_row =
            action_row.push(widget::button::suggested("Save").on_press(msg(Message::Save)));
        col = col.push(action_row);

        col.into()
    }
}

fn msg(m: Message) -> crate::pages::Message {
    crate::pages::Message::DisplayGaming(m)
}

// ── page::Page impl ───────────────────────────────────────────────────────────

impl page::AutoBind<crate::pages::Message> for Page {}

impl page::Page<crate::pages::Message> for Page {
    fn info(&self) -> page::Info {
        page::Info::new("display-gaming", "applications-games-symbolic")
            .title("Gaming")
            .description("Gamescope upscaling, frame limiting, HDR and launch options")
    }

    fn set_id(&mut self, entity: page::Entity) {
        self.entity = entity;
    }

    fn content(
        &self,
        sections: &mut slotmap::SlotMap<section::Entity, Section<crate::pages::Message>>,
    ) -> Option<page::Content> {
        Some(vec![sections.insert(gaming_view_section())])
    }

    fn on_enter(&mut self) -> Task<crate::pages::Message> {
        cosmic::task::future(async move {
            let (conf, gpu) = tokio::task::spawn_blocking(|| (read_conf(), gpu_vendor()))
                .await
                .unwrap_or_else(|e| {
                    tracing::error!("Gaming on_enter task panicked: {e:?}");
                    (GamingConf::default(), "unknown")
                });
            crate::pages::Message::DisplayGaming(Message::Loaded(conf, gpu))
        })
    }
}

fn gaming_view_section() -> Section<crate::pages::Message> {
    Section::default()
        .title("Gaming")
        .search_ignore()
        .view::<Page>(|_binder, page, _section| page.view())
}
