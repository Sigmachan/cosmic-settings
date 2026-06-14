// Copyright 2024 Kira Keller <senedato@gmail.com>
// SPDX-License-Identifier: GPL-3.0-only

//! HDR display settings — integrated as a Section of the Display page.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, column, list_column, row, settings, text, toggler};
use cosmic::{Apply, Element, Task};
use cosmic_settings_page::Section;

const BIN: &str = "/usr/local/bin/kms-hdr";
const HDR_CAL: &str = "/usr/local/lib/kms-hdr/hdr-cal.py";
const GAME_CONF: &str = "/etc/hdr-game.conf";

// ── HDR config ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HdrConf {
    pub sdr_nits:     u32,
    pub peak_nits:    u32,
    pub gamut:        u32,
    pub max_bpc:      u32,
    pub gamut_mode:   String,
    pub saturation:   u32,
    pub oled_preset:  bool,
    pub oled_dim_min: u32,
}

impl Default for HdrConf {
    fn default() -> Self {
        Self {
            sdr_nits: 203, peak_nits: 800, gamut: 100, max_bpc: 10,
            gamut_mode: "bt2020".into(), saturation: 100,
            oled_preset: false, oled_dim_min: 0,
        }
    }
}

pub fn read_conf() -> HdrConf {
    let mut c = HdrConf::default();
    if let Ok(s) = std::fs::read_to_string("/etc/kms-hdr.conf") {
        for line in s.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "SDR_NITS"     => { if let Ok(n) = v.trim().parse() { c.sdr_nits     = n; } }
                    "PEAK_NITS"    => { if let Ok(n) = v.trim().parse() { c.peak_nits    = n; } }
                    "GAMUT"        => { if let Ok(n) = v.trim().parse() { c.gamut        = n; } }
                    "MAX_BPC"      => { if let Ok(n) = v.trim().parse() { c.max_bpc      = n; } }
                    "GAMUT_MODE"   => { c.gamut_mode = v.trim().to_owned(); }
                    "SATURATION"   => { if let Ok(n) = v.trim().parse() { c.saturation   = n; } }
                    "OLED_DIM_MIN" => { if let Ok(n) = v.trim().parse() { c.oled_dim_min = n; } }
                    _ => {}
                }
            }
        }
    }
    c
}

async fn write_conf_and_apply(c: HdrConf) -> Result<(), String> {
    let status = tokio::process::Command::new("pkexec")
        .args([BIN, "--save",
               "--sdr-nits",   &c.sdr_nits.to_string(),
               "--peak-nits",  &c.peak_nits.to_string(),
               "--gamut",      &c.gamut.to_string(),
               "--bpc",        &c.max_bpc.to_string(),
               "--gamut-mode", &c.gamut_mode,
               "--saturation", &c.saturation.to_string()])
        .status().await.map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("kms-hdr exited {status}")) }
}

async fn do_reset() -> Result<(), String> {
    let status = tokio::process::Command::new("pkexec")
        .args([BIN, "reset"])
        .status().await.map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("kms-hdr reset exited {status}")) }
}

// ── NVIDIA gaming config ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NvidiaConf {
    pub smooth_motion: bool,
    pub reflex:        bool,
    pub vibrance:      i32,
    pub upscale:       String,
    pub dldsr:         bool,
    pub gs_width:      u32,
    pub gs_height:     u32,
    pub gs_fps:        u32,
}

impl Default for NvidiaConf {
    fn default() -> Self {
        Self {
            smooth_motion: true, reflex: true, vibrance: 0,
            upscale: "fsr".into(), dldsr: false,
            gs_width: 3840, gs_height: 2160, gs_fps: 120,
        }
    }
}

fn read_nvidia_conf() -> NvidiaConf {
    let mut c = NvidiaConf::default();
    if let Ok(s) = std::fs::read_to_string(GAME_CONF) {
        for line in s.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "SMOOTH_MOTION" => { c.smooth_motion = v.trim() != "0"; }
                    "REFLEX"        => { c.reflex        = v.trim() != "0"; }
                    "VIBRANCE"      => { if let Ok(n) = v.trim().parse() { c.vibrance  = n; } }
                    "UPSCALE"       => { c.upscale = v.trim().to_owned(); }
                    "DLDSR"         => { c.dldsr   = v.trim() != "0"; }
                    "GS_WIDTH"      => { if let Ok(n) = v.trim().parse() { c.gs_width  = n; } }
                    "GS_HEIGHT"     => { if let Ok(n) = v.trim().parse() { c.gs_height = n; } }
                    "GS_FPS"        => { if let Ok(n) = v.trim().parse() { c.gs_fps    = n; } }
                    _ => {}
                }
            }
        }
    }
    c
}

async fn write_nvidia_conf(c: NvidiaConf) -> Result<(), String> {
    let content = format!(
        "SMOOTH_MOTION={}\nREFLEX={}\nVIBRANCE={}\nUPSCALE={}\nDLDSR={}\nGS_WIDTH={}\nGS_HEIGHT={}\nGS_FPS={}\n",
        c.smooth_motion as u8, c.reflex as u8, c.vibrance,
        c.upscale, c.dldsr as u8, c.gs_width, c.gs_height, c.gs_fps,
    );
    tokio::fs::write(GAME_CONF, content).await
        .map_err(|e| format!("write {GAME_CONF}: {e}"))
}

// ── GPU detection ─────────────────────────────────────────────────────────────

fn gpu_vendor() -> &'static str {
    if std::path::Path::new("/dev/nvidia0").exists() { return "nvidia"; }
    for entry in std::fs::read_dir("/sys/class/drm").into_iter().flatten().flatten() {
        let vendor_path = entry.path().join("device/vendor");
        if let Ok(v) = std::fs::read_to_string(&vendor_path) {
            match v.trim() {
                "0x1002" => return "amd",
                "0x8086" => return "intel",
                _ => {}
            }
        }
    }
    "unknown"
}

// ── Display info ──────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct DisplayInfo {
    pub name:          String,
    pub connector_dir: String,
    pub hdr10:         bool,
    pub hlg:           bool,
    pub hdr10plus:     bool,
    pub dolby:         bool,
    pub bt2020:        bool,
    pub dci_p3:        bool,
    pub dsc:           bool,
    pub cec:           bool,
    pub is_oled:       bool,
    pub max_lum_nits:  u32,
    pub hdmi_ver:      Option<String>,
    pub dp_ver:        Option<String>,
}

fn find_active_connector() -> Option<(String, String)> {
    let mut found: Vec<(String, String)> = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name();
            let s = n.to_string_lossy();
            if !s.starts_with("card") || !s.contains('-') { return None; }
            let edid = format!("/sys/class/drm/{}/edid", s);
            if std::fs::read(&edid).map(|d| d.len() >= 128).unwrap_or(false) {
                Some((edid, s.to_string()))
            } else {
                None
            }
        })
        .collect();
    found.sort();
    found.into_iter().next()
}

pub fn parse_edid_blocking() -> Option<DisplayInfo> {
    let (edid_path, connector_dir) = find_active_connector()?;
    let raw = std::fs::read(&edid_path).ok()?;
    let mut info = DisplayInfo { connector_dir: connector_dir.clone(), ..Default::default() };

    // Monitor name from EDID descriptor 0xFC
    'desc: for i in (54..126usize).step_by(18) {
        if i + 17 >= raw.len() { break; }
        if raw[i..i+3] == [0x00, 0x00, 0x00] && raw[i+3] == 0xfc {
            let bytes: Vec<u8> = raw[i+5..].iter()
                .take(13).take_while(|&&b| b != b'\n').cloned().collect();
            info.name = String::from_utf8_lossy(&bytes).trim().to_owned();
            break 'desc;
        }
    }
    if info.name.is_empty() {
        info.name = connector_dir.find('-')
            .map(|p| connector_dir[p+1..].replace('-', " "))
            .unwrap_or_else(|| "Display".into());
    }

    // OLED detection: EDID product name or sysfs panel_type
    info.is_oled = info.name.to_ascii_lowercase().contains("oled");
    if !info.is_oled {
        let panel_type_path = format!("/sys/class/drm/{}/panel_type", connector_dir);
        if let Ok(pt) = std::fs::read_to_string(&panel_type_path) {
            info.is_oled = pt.to_ascii_lowercase().contains("oled");
        }
    }

    // CEA-861 extension blocks
    let mut bs = 128usize;
    while bs + 128 <= raw.len() {
        if raw[bs] != 0x02 { bs += 128; continue; }
        let dtd = raw[bs + 2] as usize;
        let mut i = 4usize;
        while i < dtd && bs + i < raw.len() {
            let tag    = (raw[bs + i] >> 5) & 0x7;
            let length = (raw[bs + i] & 0x1f) as usize;
            if bs + i + 1 + length > raw.len() { break; }
            let data = &raw[bs + i + 1 .. bs + i + 1 + length];

            match tag {
                7 if !data.is_empty() => {
                    let payload = &data[1..];
                    match data[0] {
                        6 if !payload.is_empty() => {
                            info.hdr10 = payload[0] & 0x04 != 0;
                            info.hlg   = payload[0] & 0x08 != 0;
                            if payload.len() > 2 && payload[2] != 0 {
                                info.max_lum_nits =
                                    (50.0 * 2f64.powf(payload[2] as f64 / 32.0)) as u32;
                            }
                        }
                        5 if !payload.is_empty() => {
                            info.bt2020 = payload[0] & 0x80 != 0;
                            info.dci_p3 = payload[0] & 0x02 != 0;
                        }
                        13 => { info.hdr10plus = true; }
                        1 if payload.len() >= 3 => {
                            let oui = u32::from_le_bytes([payload[0], payload[1], payload[2], 0]);
                            if oui == 0x0000_D046 { info.dolby = true; }
                        }
                        _ => {}
                    }
                }
                3 if data.len() >= 3 => {
                    let oui = u32::from_le_bytes([data[0], data[1], data[2], 0]);
                    match oui {
                        0x0000_D046 => { info.dolby = true; }
                        0x0000_0C03 => {
                            if info.hdmi_ver.is_none() {
                                info.hdmi_ver = Some("HDMI 1.4".into());
                            }
                        }
                        0x00C4_5D00 => {
                            let max_tmds_mhz = if data.len() >= 5 { data[4] as u32 * 5 } else { 0 };
                            info.hdmi_ver = Some(if max_tmds_mhz >= 600 {
                                "HDMI 2.1".into()
                            } else {
                                "HDMI 2.0".into()
                            });
                            if data.len() >= 9 && data[8] & 0x80 != 0 { info.dsc = true; }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            i += 1 + length;
        }
        bs += 128;
    }

    if std::path::Path::new(&format!("/sys/class/drm/{}/dsc_enable", connector_dir)).exists() {
        info.dsc = true;
    }

    if connector_dir.contains("-DP-") || connector_dir.contains("-eDP-") {
        if let Ok(dpcd) = std::fs::read(format!("/sys/class/drm/{}/dpcd", connector_dir)) {
            if !dpcd.is_empty() {
                info.dp_ver = Some(match dpcd[0] {
                    0x10 => "DP 1.0".into(),
                    0x11 => "DP 1.1".into(),
                    0x12 => "DP 1.2".into(),
                    0x13 => "DP 1.3".into(),
                    0x14 => "DP 1.4".into(),
                    v if v >= 0x20 => "DP 2.x (UHBR)".into(),
                    v => format!("DP (DPCD {v:#04x})"),
                });
            }
        }
    }

    info.cec = std::path::Path::new("/dev/cec0").exists();
    Some(info)
}

fn service_active() -> bool {
    std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", "kms-hdr.service"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── OLED auto-dim service ─────────────────────────────────────────────────────

async fn setup_oled_dim(minutes: u32) -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let svc_dir  = format!("{home}/.config/systemd/user");
    let svc_path = format!("{svc_dir}/kms-hdr-dim.service");

    if minutes == 0 {
        let _ = std::fs::remove_file(&svc_path);
        tokio::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status().await.map_err(|e| e.to_string())?;
        return Ok(());
    }

    let secs = minutes * 60;
    let content = format!(
        "[Unit]\nDescription=kms-hdr OLED auto-dim\n\n\
         [Service]\nType=simple\n\
         ExecStart=/usr/bin/swayidle -w timeout {secs} \"pkexec {BIN} --dim-to 50\" resume \"pkexec {BIN}\"\n\
         Restart=always\n\n\
         [Install]\nWantedBy=default.target\n"
    );

    tokio::fs::create_dir_all(&svc_dir).await.map_err(|e| e.to_string())?;
    tokio::fs::write(&svc_path, content).await.map_err(|e| e.to_string())?;
    tokio::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status().await.map_err(|e| e.to_string())?;
    tokio::process::Command::new("systemctl")
        .args(["--user", "enable", "--now", "kms-hdr-dim.service"])
        .status().await.map_err(|e| e.to_string())?;
    Ok(())
}

// ── Calibration patterns ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalibPattern {
    Black, DarkGray, Gray50, White, Red, Green, Blue, SdrHdrSplit,
}

impl CalibPattern {
    fn label(self) -> &'static str {
        match self {
            Self::Black       => "Black",
            Self::DarkGray    => "5% Gray",
            Self::Gray50      => "50% Gray",
            Self::White       => "White",
            Self::Red         => "Red",
            Self::Green       => "Green",
            Self::Blue        => "Blue",
            Self::SdrHdrSplit => "SDR│HDR",
        }
    }
    fn arg(self) -> &'static str {
        match self {
            Self::Black       => "black",
            Self::DarkGray    => "darkgray",
            Self::Gray50      => "gray50",
            Self::White       => "white",
            Self::Red         => "red",
            Self::Green       => "green",
            Self::Blue        => "blue",
            Self::SdrHdrSplit => "sdr_hdr",
        }
    }
}

const GS_RESOLUTIONS: &[(u32, u32, &str)] = &[
    (1920, 1080, "1920 × 1080  (1080p)"),
    (2560, 1440, "2560 × 1440  (1440p)"),
    (3840, 2160, "3840 × 2160  (4K UHD)"),
    (5120, 2880, "5120 × 2880  (5K)"),
    (7680, 4320, "7680 × 4320  (8K)"),
];

const UPSCALE_MODES: &[&str] = &["fsr", "nis", "integer", "nearest"];
const UPSCALE_LABELS: &[&str] = &["FSR 1.0 (AMD FidelityFX)", "NIS (NVIDIA ImageScaling)", "Integer scale", "Nearest-neighbour"];

// ── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    Loaded { enabled: bool, display: Option<DisplayInfo>, gpu: &'static str },
    HdrToggle(bool),
    SdrNits(u32),
    PeakNits(u32),
    Gamut(u32),
    GamutMode(usize),
    Saturation(u32),
    BitDepth(usize),
    Apply,
    Reset,
    Applied(Result<(), String>),
    ShowCalPat(CalibPattern),
    CalibrateHdr,
    CloseCalPat,
    // OLED Care
    OledPreset(bool),
    OledDimTimeout(u32),
    OledDimApplied(Result<(), String>),
    // NVIDIA Gaming
    NvSmoothMotion(bool),
    NvReflex(bool),
    NvVibrance(i32),
    NvUpscale(usize),
    NvDldsr(bool),
    NvGsResolution(u32, u32),
    NvGsFps(u32),
    NvApply,
    NvApplied(Result<(), String>),
}

// ── Page state ────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct Page {
    pub conf:            HdrConf,
    pub nvidia_conf:     NvidiaConf,
    pub hdr_enabled:     bool,
    pub display:         Option<DisplayInfo>,
    pub display_is_oled: bool,
    pub gpu_vendor:      &'static str,
    pub is_nvidia:       bool,
    pub status:          Option<String>,
    pub cal_child:       Option<std::process::Child>,
}

impl Page {
    pub fn on_enter(&self) -> Task<crate::pages::Message> {
        cosmic::task::future(async move {
            let (enabled, display, gpu) = tokio::task::spawn_blocking(|| {
                (service_active(), parse_edid_blocking(), gpu_vendor())
            }).await.unwrap_or((false, None, "unknown"));
            crate::pages::Message::Displays(
                super::Message::Hdr(Message::Loaded { enabled, display, gpu })
            )
        })
    }

    pub fn update(&mut self, message: Message) -> Task<crate::app::Message> {
        match message {
            Message::Loaded { enabled, display, gpu } => {
                self.conf        = read_conf();
                self.nvidia_conf = read_nvidia_conf();
                self.hdr_enabled = enabled;
                self.gpu_vendor  = gpu;
                self.is_nvidia   = gpu == "nvidia";
                if let Some(ref d) = display { self.display_is_oled = d.is_oled; }
                self.display     = display;
            }

            Message::HdrToggle(on) => {
                self.hdr_enabled = on;
                self.status = Some(if on { "Enabling HDR…".into() } else { "Resetting to SDR…".into() });
                let c = self.conf.clone();
                return cosmic::task::future(async move {
                    let result = if on { write_conf_and_apply(c).await } else { do_reset().await };
                    crate::app::Message::from(super::Message::Hdr(Message::Applied(result)))
                });
            }

            Message::SdrNits(v)   => { self.conf.sdr_nits  = v; }
            Message::PeakNits(v)  => { self.conf.peak_nits = v; }
            Message::Gamut(v)     => { self.conf.gamut      = v; }
            Message::Saturation(v) => { self.conf.saturation = v; }
            Message::GamutMode(i) => {
                self.conf.gamut_mode = ["bt2020", "dci-p3", "srgb"][i.min(2)].into();
            }
            Message::BitDepth(i) => {
                self.conf.max_bpc = [8u32, 10, 12][i.min(2)];
            }

            Message::Apply => {
                self.status = Some("Applying…".into());
                let c = self.conf.clone();
                return cosmic::task::future(async move {
                    let result = write_conf_and_apply(c).await;
                    crate::app::Message::from(super::Message::Hdr(Message::Applied(result)))
                });
            }
            Message::Reset => {
                self.hdr_enabled = false;
                self.status = Some("Resetting…".into());
                return cosmic::task::future(async move {
                    let result = do_reset().await;
                    crate::app::Message::from(super::Message::Hdr(Message::Applied(result)))
                });
            }
            Message::Applied(Ok(())) => { self.status = Some("Applied ✓".into()); }
            Message::Applied(Err(e)) => { self.status = Some(format!("Error: {e}")); }

            Message::ShowCalPat(pat) => {
                if let Some(mut c) = self.cal_child.take() { let _ = c.kill(); }
                match std::process::Command::new("python3").args([HDR_CAL, pat.arg()]).spawn() {
                    Ok(child) => { self.cal_child = Some(child); }
                    Err(e)    => { self.status = Some(format!("hdr-cal: {e}")); }
                }
            }
            Message::CalibrateHdr => {
                if let Some(mut c) = self.cal_child.take() { let _ = c.kill(); }
                let c = self.conf.clone();
                match std::process::Command::new("python3")
                    .args([HDR_CAL, "--calibrate",
                           "--sdr-nits",   &c.sdr_nits.to_string(),
                           "--peak-nits",  &c.peak_nits.to_string(),
                           "--gamut",      &c.gamut.to_string(),
                           "--bpc",        &c.max_bpc.to_string(),
                           "--gamut-mode", &c.gamut_mode])
                    .spawn()
                {
                    Ok(child) => { self.cal_child = Some(child); }
                    Err(e)    => { self.status = Some(format!("hdr-cal: {e}")); }
                }
            }
            Message::CloseCalPat => {
                if let Some(mut c) = self.cal_child.take() { let _ = c.kill(); }
            }

            // OLED Care
            Message::OledPreset(on) => {
                self.conf.oled_preset = on;
                if on {
                    self.conf.sdr_nits  = 150;
                    self.conf.peak_nits = 600;
                } else {
                    self.conf.sdr_nits  = 203;
                    self.conf.peak_nits = 800;
                }
            }
            Message::OledDimTimeout(minutes) => {
                self.conf.oled_dim_min = minutes;
                self.status = Some(if minutes == 0 { "OLED auto-dim disabled".into() }
                                   else { format!("Setting auto-dim to {minutes} min…") });
                return cosmic::task::future(async move {
                    let result = setup_oled_dim(minutes).await;
                    crate::app::Message::from(super::Message::Hdr(Message::OledDimApplied(result)))
                });
            }
            Message::OledDimApplied(Ok(())) => {
                self.status = Some("OLED auto-dim updated ✓".into());
            }
            Message::OledDimApplied(Err(e)) => {
                self.status = Some(format!("OLED dim error: {e}"));
            }

            // NVIDIA Gaming
            Message::NvSmoothMotion(v)      => { self.nvidia_conf.smooth_motion = v; }
            Message::NvReflex(v)            => { self.nvidia_conf.reflex        = v; }
            Message::NvVibrance(v)          => { self.nvidia_conf.vibrance      = v; }
            Message::NvDldsr(v)             => { self.nvidia_conf.dldsr         = v; }
            Message::NvGsResolution(w, h)   => { self.nvidia_conf.gs_width = w; self.nvidia_conf.gs_height = h; }
            Message::NvGsFps(v)             => { self.nvidia_conf.gs_fps        = v; }
            Message::NvUpscale(i)           => {
                self.nvidia_conf.upscale = UPSCALE_MODES[i.min(UPSCALE_MODES.len() - 1)].into();
            }
            Message::NvApply => {
                self.status = Some("Saving NVIDIA gaming config…".into());
                let c = self.nvidia_conf.clone();
                return cosmic::task::future(async move {
                    let result = write_nvidia_conf(c).await;
                    crate::app::Message::from(super::Message::Hdr(Message::NvApplied(result)))
                });
            }
            Message::NvApplied(Ok(())) => { self.status = Some("NVIDIA config saved ✓".into()); }
            Message::NvApplied(Err(e)) => { self.status = Some(format!("NVIDIA config error: {e}")); }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, crate::pages::Message> {
        let theme = cosmic::theme::active();
        let sp = &theme.cosmic().spacing;
        let mut col = column::with_capacity(20).spacing(sp.space_m);

        // ── Display capabilities ──────────────────────────────────────────────
        if let Some(ref d) = self.display {
            let cap = |label: &'static str, ok: bool| {
                text::caption(if ok { format!("{label} ✓") } else { format!("{label} —") })
            };

            let hdr_row = row::with_capacity(4)
                .spacing(sp.space_xs)
                .push(cap("HDR10",        d.hdr10))
                .push(cap("HLG",          d.hlg))
                .push(cap("HDR10+",       d.hdr10plus))
                .push(cap("Dolby Vision", d.dolby));

            let feat_row = row::with_capacity(5)
                .spacing(sp.space_xs)
                .push(cap("BT.2020",  d.bt2020))
                .push(cap("DCI-P3",   d.dci_p3))
                .push(cap("DSC",      d.dsc))
                .push(cap("HDMI-CEC", d.cec))
                .push(cap("OLED",     d.is_oled));

            let iface = d.hdmi_ver.as_deref().or(d.dp_ver.as_deref()).unwrap_or("Unknown interface");
            let desc = if d.max_lum_nits > 0 {
                format!("{iface} · EDID peak {} nits", d.max_lum_nits)
            } else {
                format!("{iface} · peak luminance not specified in EDID")
            };

            col = col.push(
                list_column().add(
                    settings::item::builder(d.name.as_str())
                        .description(desc)
                        .control(
                            column::with_capacity(2)
                                .spacing(sp.space_xxs)
                                .push(hdr_row)
                                .push(feat_row)
                        ),
                ),
            );
        }

        // ── GPU badge ─────────────────────────────────────────────────────────
        {
            let gpu_desc = match self.gpu_vendor {
                "amd"    => "AMD  ·  Full pipeline: DEGAMMA + CTM + GAMMA + saturation",
                "intel"  => "Intel  ·  Full pipeline: DEGAMMA + CTM + GAMMA + saturation",
                "nvidia" => "NVIDIA  ·  Gamma-only on desktop  ·  Full HDR + gaming features via Gamescope (see below)",
                _        => "GPU vendor unknown",
            };
            col = col.push(
                text::caption(gpu_desc)
                    .apply(widget::container)
                    .padding([sp.space_xxs, sp.space_xs]),
            );
        }

        // ── OLED Care (only when OLED detected) ───────────────────────────────
        if self.display_is_oled {
            let dim_label = if self.conf.oled_dim_min == 0 {
                "Off".to_string()
            } else {
                format!("{} min", self.conf.oled_dim_min)
            };

            col = col.push(text::heading("OLED Care"));
            col = col.push(
                list_column()
                    .add(settings::item::builder("Longevity Preset")
                        .description("SDR 150 nit / peak 600 nit — reduces panel stress in daily use")
                        .control(toggler(self.conf.oled_preset)
                            .on_toggle(|v| msg(Message::OledPreset(v)))))
                    .add(settings::item::builder("Auto-dim after idle")
                        .description("Dims to 50 nit when no input detected — requires swayidle")
                        .control(
                            row::with_capacity(2).spacing(sp.space_s).align_y(Alignment::Center)
                                .push(widget::slider(0..=60u32, self.conf.oled_dim_min,
                                                     |v| msg(Message::OledDimTimeout(v)))
                                    .step(5u32).width(Length::Fill))
                                .push(text::body(dim_label)
                                    .apply(widget::container).width(Length::Fixed(52.0))),
                        ))
                    .add(settings::item::builder("Pixel shift")
                        .description("Configure in COSMIC Display compositor settings")
                        .control(widget::Space::new().width(Length::Shrink))),
            );
        }

        // ── HDR toggle ────────────────────────────────────────────────────────
        col = col.push(list_column().add(
            settings::item::builder("Enable HDR10")
                .description("BT.2020 wide-colour + PQ (ST 2084) tone mapping via KMS")
                .control(toggler(self.hdr_enabled).on_toggle(|v| msg(Message::HdrToggle(v)))),
        ));

        // ── Brightness ────────────────────────────────────────────────────────
        let sdr_row = settings::item::builder("SDR White Point")
            .description("Brightness of desktop and SDR content in HDR mode (ITU-R BT.2408: 203 nits)")
            .control(
                row::with_capacity(2).spacing(sp.space_s).align_y(Alignment::Center)
                    .push(widget::slider(80..=400u32, self.conf.sdr_nits, |v| msg(Message::SdrNits(v)))
                        .width(Length::Fill))
                    .push(text::body(format!("{} nits", self.conf.sdr_nits))
                        .apply(widget::container).width(Length::Fixed(76.0))),
            );

        let peak_row = settings::item::builder("Display Peak Luminance")
            .description("Your display's maximum HDR nits — used for HDR10 metadata signaling")
            .control(
                row::with_capacity(2).spacing(sp.space_s).align_y(Alignment::Center)
                    .push(widget::slider(400..=1600u32, self.conf.peak_nits, |v| msg(Message::PeakNits(v)))
                        .step(10u32).width(Length::Fill))
                    .push(text::body(format!("{} nits", self.conf.peak_nits))
                        .apply(widget::container).width(Length::Fixed(76.0))),
            );

        col = col.push(list_column().add(sdr_row).add(peak_row));

        // ── Colour gamut ──────────────────────────────────────────────────────
        let colour_heading = if self.is_nvidia { "Colour  (AMD / Intel only)" } else { "Colour" };
        col = col.push(text::heading(colour_heading));

        let gamut_opts = vec![
            "BT.2020  (full wide colour — UHDTV / DCI cinemas)".to_string(),
            "DCI-P3 D65  (Apple / cinema wide-gamut, D65 white)".to_string(),
            "sRGB  (no gamut expansion — tone map only)".to_string(),
        ];
        let gamut_sel = match self.conf.gamut_mode.as_str() {
            "dci-p3" => Some(1usize),
            "srgb"   => Some(2usize),
            _        => Some(0usize),
        };

        let expansion_row = settings::item::builder("Gamut Expansion")
            .description("0% = sRGB passthrough  ·  100% = full target gamut via 3×3 CTM (AMD/Intel only)")
            .control(
                row::with_capacity(2).spacing(sp.space_s).align_y(Alignment::Center)
                    .push(widget::slider(0..=100u32, self.conf.gamut, |v| msg(Message::Gamut(v)))
                        .width(Length::Fill))
                    .push(text::body(format!("{}%", self.conf.gamut))
                        .apply(widget::container).width(Length::Fixed(48.0))),
            );

        col = col.push(
            list_column()
                .add(settings::item::builder("Target Colour Space")
                    .description("Colour space the CTM matrix maps sRGB primaries into")
                    .control(widget::dropdown(gamut_opts, gamut_sel, |i| msg(Message::GamutMode(i)))
                        .width(Length::Fixed(290.0))))
                .add(expansion_row),
        );

        // ── Color intensity ───────────────────────────────────────────────────
        let intensity_heading = if self.is_nvidia { "Color Intensity  (AMD / Intel only)" } else { "Color Intensity" };
        col = col.push(text::heading(intensity_heading));

        let sat_row = settings::item::builder("Saturation")
            .description("Boost or reduce colour vividness · 100% = neutral · applied via BT.709 CTM")
            .control(
                row::with_capacity(2).spacing(sp.space_s).align_y(Alignment::Center)
                    .push(widget::slider(50..=200u32, self.conf.saturation, |v| msg(Message::Saturation(v)))
                        .step(5u32).width(Length::Fill))
                    .push(text::body(format!("{}%", self.conf.saturation))
                        .apply(widget::container).width(Length::Fixed(52.0))),
            );
        col = col.push(list_column().add(sat_row));

        // ── Output format ─────────────────────────────────────────────────────
        let bpc_opts = vec![
            "8 bpc  (legacy / compatibility)".to_string(),
            "10 bpc  (HDR10 — recommended)".to_string(),
            "12 bpc  (HDR+ / reference monitors)".to_string(),
        ];
        let bpc_sel = match self.conf.max_bpc { 8 => Some(0usize), 12 => Some(2), _ => Some(1) };

        col = col.push(list_column().add(
            settings::item::builder("Output Bit Depth")
                .description("Requested via max_requested_bpc connector property")
                .control(widget::dropdown(bpc_opts, bpc_sel, |i| msg(Message::BitDepth(i)))
                    .width(Length::Fixed(290.0))),
        ));

        // ── NVIDIA Gaming (only when NVIDIA GPU detected) ─────────────────────
        if self.is_nvidia {
            col = col.push(text::heading("NVIDIA Gaming  (Gamescope / hdr-game)"));

            let res_opts: Vec<String> = GS_RESOLUTIONS.iter()
                .map(|&(_, _, label)| label.to_string()).collect();
            let res_sel = GS_RESOLUTIONS.iter().position(|&(w, h, _)| {
                w == self.nvidia_conf.gs_width && h == self.nvidia_conf.gs_height
            });

            let upscale_opts: Vec<String> = UPSCALE_LABELS.iter().map(|s| s.to_string()).collect();
            let upscale_sel  = UPSCALE_MODES.iter().position(|&m| m == self.nvidia_conf.upscale);

            let fps_row = settings::item::builder("Frame Rate Cap")
                .description("Gamescope --framerate-limit passed to hdr-game")
                .control(
                    row::with_capacity(2).spacing(sp.space_s).align_y(Alignment::Center)
                        .push(widget::slider(30..=360u32, self.nvidia_conf.gs_fps, |v| msg(Message::NvGsFps(v)))
                            .step(5u32).width(Length::Fill))
                        .push(text::body(format!("{} fps", self.nvidia_conf.gs_fps))
                            .apply(widget::container).width(Length::Fixed(64.0))),
                );

            let vibrance_row = settings::item::builder("Digital Vibrance")
                .description("nvibrant ioctl — 0 = neutral, 1023 = maximum saturation")
                .control(
                    row::with_capacity(2).spacing(sp.space_s).align_y(Alignment::Center)
                        .push(widget::slider(-1024..=1023i32, self.nvidia_conf.vibrance, |v| msg(Message::NvVibrance(v)))
                            .width(Length::Fill))
                        .push(text::body(format!("{}", self.nvidia_conf.vibrance))
                            .apply(widget::container).width(Length::Fixed(52.0))),
                );

            col = col.push(
                list_column()
                    .add(settings::item::builder("RTX Smooth Motion")
                        .description("NVPRESENT_ENABLE_SMOOTH_MOTION — frame interpolation (driver 575+)")
                        .control(toggler(self.nvidia_conf.smooth_motion)
                            .on_toggle(|v| msg(Message::NvSmoothMotion(v)))))
                    .add(settings::item::builder("NVIDIA Reflex (Low Latency)")
                        .description("PROTON_ENABLE_NVAPI + DXVK_ENABLE_NVAPI — requires Proton/DXVK game")
                        .control(toggler(self.nvidia_conf.reflex)
                            .on_toggle(|v| msg(Message::NvReflex(v)))))
                    .add(settings::item::builder("DLDSR 2.25×")
                        .description("Deep Learning DSR — renders at 1.5× linear resolution, downscales to native")
                        .control(toggler(self.nvidia_conf.dldsr)
                            .on_toggle(|v| msg(Message::NvDldsr(v)))))
                    .add(vibrance_row)
                    .add(settings::item::builder("Gamescope Output Resolution")
                        .description("Virtual display resolution Gamescope presents to the game")
                        .control(widget::dropdown(res_opts, res_sel, |i| {
                            let (w, h, _) = GS_RESOLUTIONS[i.min(GS_RESOLUTIONS.len() - 1)];
                            msg(Message::NvGsResolution(w, h))
                        }).width(Length::Fixed(260.0))))
                    .add(settings::item::builder("Upscaling Mode")
                        .description("Upscaling algorithm used by Gamescope when rendering below native")
                        .control(widget::dropdown(upscale_opts, upscale_sel, |i| msg(Message::NvUpscale(i)))
                            .width(Length::Fixed(260.0))))
                    .add(fps_row),
            );

            col = col.push(
                widget::button::suggested("Save NVIDIA config").on_press(msg(Message::NvApply))
            );
        }

        // ── Calibration ───────────────────────────────────────────────────────
        const PATTERNS: &[CalibPattern] = &[
            CalibPattern::Black, CalibPattern::DarkGray, CalibPattern::Gray50,
            CalibPattern::White, CalibPattern::Red, CalibPattern::Green,
            CalibPattern::Blue,  CalibPattern::SdrHdrSplit,
        ];

        let mut pat_row = row::with_capacity(10).spacing(sp.space_xxs).align_y(Alignment::Center);
        for &p in PATTERNS {
            pat_row = pat_row.push(
                widget::button::standard(p.label()).on_press(msg(Message::ShowCalPat(p)))
            );
        }
        if self.cal_child.is_some() {
            pat_row = pat_row.push(
                widget::button::destructive("✕ Close").on_press(msg(Message::CloseCalPat))
            );
        }

        col = col.push(
            list_column()
                .add(settings::item::builder("Calibrate HDR")
                    .description("Adjust SDR white brightness interactively against a reference pattern")
                    .control(widget::button::suggested("Calibrate…").on_press(msg(Message::CalibrateHdr))))
                .add(settings::item::builder("Test Patterns")
                    .description("Full-screen colour fields — press Esc or click to close")
                    .control(pat_row)),
        );

        // ── Status + action row ───────────────────────────────────────────────
        let mut action_row = row::with_capacity(3)
            .spacing(sp.space_s)
            .align_y(Alignment::Center)
            .padding([0, 0, sp.space_s, 0]);

        if let Some(ref s) = self.status {
            action_row = action_row.push(
                text::caption(s.as_str()).apply(widget::container).width(Length::Fill)
            );
        } else {
            action_row = action_row.push(widget::Space::new().width(Length::Fill));
        }

        action_row = action_row
            .push(widget::button::destructive("Reset to SDR").on_press(msg(Message::Reset)))
            .push(widget::button::suggested("Apply").on_press(msg(Message::Apply)));

        col = col.push(action_row);
        col.into()
    }
}

fn msg(m: Message) -> crate::pages::Message {
    crate::pages::Message::Displays(super::Message::Hdr(m))
}

// ── Section factory ───────────────────────────────────────────────────────────

pub fn section() -> Section<crate::pages::Message> {
    Section::default()
        .title("HDR")
        .search_ignore()
        .view::<super::Page>(|_binder, page, _section| {
            settings::view_column(vec![
                text::heading("HDR & Wide Colour").into(),
                page.hdr.view(),
            ])
            .into()
        })
}
