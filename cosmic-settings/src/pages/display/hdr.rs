// Copyright 2024 Kira Keller <senedato@gmail.com>
// SPDX-License-Identifier: GPL-3.0-only

//! HDR display settings — sub-page of the Display page.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, column, list_column, row, settings, text, toggler};
use cosmic::{Apply, Element, Task};
use cosmic_settings_page::{self as page, Section, section};

const BIN: &str = "/usr/local/bin/kms-hdr";
const HDR_CAL: &str = "/usr/local/lib/kms-hdr/hdr-cal.py";

// ── HDR config ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HdrConf {
    pub sdr_nits: u32,
    pub peak_nits: u32,
    pub gamut: u32,
    pub max_bpc: u32,
    pub gamut_mode: String,
    pub saturation: u32,
    pub midtone_gamma: u32,
    pub oled_preset: bool,
    pub oled_dim_min: u32,
    pub force_oled: bool,
    /// Automatic mode: the daemon enables HDR from EDID with no manual fiddling.
    pub auto_mode: bool,
}

impl Default for HdrConf {
    fn default() -> Self {
        Self {
            sdr_nits: 203,
            peak_nits: 800,
            gamut: 100,
            max_bpc: 10,
            gamut_mode: "bt2020".into(),
            saturation: 100,
            midtone_gamma: 100,
            oled_preset: false,
            oled_dim_min: 0,
            force_oled: false,
            auto_mode: true,
        }
    }
}

pub fn read_conf() -> HdrConf {
    let mut c = HdrConf::default();
    if let Ok(s) = std::fs::read_to_string("/etc/kms-hdr.conf") {
        for line in s.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "SDR_NITS" => {
                        if let Ok(n) = v.trim().parse() {
                            c.sdr_nits = n;
                        }
                    }
                    "PEAK_NITS" => {
                        if let Ok(n) = v.trim().parse() {
                            c.peak_nits = n;
                        }
                    }
                    "GAMUT" => {
                        if let Ok(n) = v.trim().parse() {
                            c.gamut = n;
                        }
                    }
                    "MAX_BPC" => {
                        if let Ok(n) = v.trim().parse() {
                            c.max_bpc = n;
                        }
                    }
                    "GAMUT_MODE" => {
                        let m = v.trim();
                        if ["bt2020", "dci-p3", "srgb"].contains(&m) {
                            c.gamut_mode = m.to_owned();
                        }
                    }
                    "SATURATION" => {
                        if let Ok(n) = v.trim().parse() {
                            c.saturation = n;
                        }
                    }
                    "MIDTONE_GAMMA" => {
                        if let Ok(n) = v.trim().parse() {
                            c.midtone_gamma = n;
                        }
                    }
                    "OLED_DIM_MIN" => {
                        if let Ok(n) = v.trim().parse() {
                            c.oled_dim_min = n;
                        }
                    }
                    "FORCE_OLED" => {
                        c.force_oled = v.trim() == "1";
                    }
                    "AUTO" => {
                        c.auto_mode = v.trim() != "0";
                    }
                    _ => {}
                }
            }
        }
    }
    c
}

fn daemon_alive() -> bool {
    std::fs::read_to_string("/run/kms-hdr.pid")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(|pid| std::path::Path::new(&format!("/proc/{pid}")).exists())
        .unwrap_or(false)
}

fn nits_to_pq_percent(nits: u32) -> f64 {
    const M1: f64 = 0.1593017578125;
    const M2: f64 = 78.84375;
    const C1: f64 = 0.8359375;
    const C2: f64 = 18.8515625;
    const C3: f64 = 18.6875;
    let y = (nits as f64 / 10_000.0).clamp(0.0, 1.0);
    let ym = y.powf(M1);
    ((C1 + C2 * ym) / (1.0 + C3 * ym)).max(0.0).powf(M2) * 100.0
}

async fn write_conf_and_apply(c: HdrConf) -> Result<(), String> {
    let force_oled_s = if c.force_oled {
        "1".to_string()
    } else {
        "0".to_string()
    };
    let args = [
        "--sdr-nits",
        &*c.sdr_nits.to_string(),
        "--peak-nits",
        &*c.peak_nits.to_string(),
        "--gamut",
        &*c.gamut.to_string(),
        "--bpc",
        &*c.max_bpc.to_string(),
        "--gamut-mode",
        &*c.gamut_mode,
        "--saturation",
        &*c.saturation.to_string(),
        "--midtone-gamma",
        &*c.midtone_gamma.to_string(),
        "--oled-dim-min",
        &*c.oled_dim_min.to_string(),
        "--force-oled",
        &*force_oled_s,
    ];
    if daemon_alive() {
        // Fast path: write conf then signal daemon — no VT switch
        let mut save_args = vec![BIN, "--save-only"];
        save_args.extend_from_slice(&args);
        let s = tokio::process::Command::new("pkexec")
            .args(&save_args)
            .status()
            .await
            .map_err(|e| e.to_string())?;
        if !s.success() {
            return Err(format!("kms-hdr --save-only exited {s}"));
        }
        let r = tokio::process::Command::new("pkexec")
            .args([BIN, "--reload"])
            .status()
            .await
            .map_err(|e| e.to_string())?;
        if r.success() {
            return Ok(());
        }
        // daemon died between check and reload — fall through to direct apply
    }
    let mut full_args = vec![BIN, "--save"];
    full_args.extend_from_slice(&args);
    let s = tokio::process::Command::new("pkexec")
        .args(&full_args)
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if s.success() {
        Ok(())
    } else {
        Err(format!("kms-hdr exited {s}"))
    }
}

async fn do_reset() -> Result<(), String> {
    let status = tokio::process::Command::new("pkexec")
        .args([BIN, "reset"])
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("kms-hdr reset exited {status}"))
    }
}

/// Persist Automatic/Manual mode, and when switching to Automatic engage HDR
/// right away if the display is HDR-capable (the daemon would otherwise only
/// re-evaluate on the next hotplug/login).
async fn set_auto_mode(on: bool) -> Result<(), String> {
    let flag = if on { "1" } else { "0" };
    let s = tokio::process::Command::new("pkexec")
        .args([BIN, "--set-auto", flag, "--save-only"])
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if !s.success() {
        return Err(format!("kms-hdr --set-auto exited {s}"));
    }
    if on {
        // Engage now if appropriate; --auto is a no-op on SDR displays.
        let _ = tokio::process::Command::new("pkexec")
            .args([BIN, "--auto"])
            .status()
            .await;
    }
    Ok(())
}

// ── GPU detection ─────────────────────────────────────────────────────────────

fn gpu_vendor() -> &'static str {
    if std::path::Path::new("/dev/nvidia0").exists() {
        return "nvidia";
    }
    for entry in std::fs::read_dir("/sys/class/drm")
        .into_iter()
        .flatten()
        .flatten()
    {
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
    pub name: String,
    pub connector_dir: String,
    pub hdr10: bool,
    pub hlg: bool,
    pub hdr10plus: bool,
    pub dolby: bool,
    pub bt2020: bool,
    pub dci_p3: bool,
    pub dsc: bool,
    pub cec: bool,
    pub is_oled: bool,
    pub max_lum_nits: u32,
    pub hdmi_ver: Option<String>,
    pub dp_ver: Option<String>,
}

fn find_active_connector() -> Option<(String, String)> {
    let mut found: Vec<(String, String)> = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name();
            let s = n.to_string_lossy();
            if !s.starts_with("card") || !s.contains('-') {
                return None;
            }
            let edid = format!("/sys/class/drm/{}/edid", s);
            if std::fs::read(&edid)
                .map(|d| d.len() >= 128)
                .unwrap_or(false)
            {
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
    let mut info = DisplayInfo {
        connector_dir: connector_dir.clone(),
        ..Default::default()
    };

    // Monitor name from EDID descriptor 0xFC
    'desc: for i in (54..126usize).step_by(18) {
        if i + 17 >= raw.len() {
            break;
        }
        if raw[i..i + 3] == [0x00, 0x00, 0x00] && raw[i + 3] == 0xfc {
            let bytes: Vec<u8> = raw[i + 5..]
                .iter()
                .take(13)
                .take_while(|&&b| b != b'\n')
                .cloned()
                .collect();
            info.name = String::from_utf8_lossy(&bytes).trim().to_owned();
            break 'desc;
        }
    }
    if info.name.is_empty() {
        info.name = connector_dir
            .find('-')
            .map(|p| connector_dir[p + 1..].replace('-', " "))
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
        if raw[bs] != 0x02 {
            bs += 128;
            continue;
        }
        let dtd = raw[bs + 2] as usize;
        let mut i = 4usize;
        while i < dtd && bs + i < raw.len() {
            let tag = (raw[bs + i] >> 5) & 0x7;
            let length = (raw[bs + i] & 0x1f) as usize;
            if bs + i + 1 + length > raw.len() {
                break;
            }
            let data = &raw[bs + i + 1..bs + i + 1 + length];

            match tag {
                7 if !data.is_empty() => {
                    let payload = &data[1..];
                    match data[0] {
                        6 if !payload.is_empty() => {
                            info.hdr10 = payload[0] & 0x04 != 0;
                            info.hlg = payload[0] & 0x08 != 0;
                            if payload.len() > 2 && payload[2] != 0 {
                                info.max_lum_nits =
                                    (50.0 * 2f64.powf(payload[2] as f64 / 32.0)) as u32;
                            }
                        }
                        5 if !payload.is_empty() => {
                            info.bt2020 = payload[0] & 0x80 != 0;
                            info.dci_p3 = payload[0] & 0x02 != 0;
                        }
                        13 => {
                            info.hdr10plus = true;
                        }
                        1 if payload.len() >= 3 => {
                            let oui = u32::from_le_bytes([payload[0], payload[1], payload[2], 0]);
                            if oui == 0x0000_D046 {
                                info.dolby = true;
                            }
                        }
                        _ => {}
                    }
                }
                3 if data.len() >= 3 => {
                    let oui = u32::from_le_bytes([data[0], data[1], data[2], 0]);
                    match oui {
                        0x0000_D046 => {
                            info.dolby = true;
                        }
                        0x0000_0C03 => {
                            if info.hdmi_ver.is_none() {
                                info.hdmi_ver = Some("HDMI 1.4".into());
                            }
                        }
                        0x00C4_5D00 => {
                            let max_tmds_mhz = if data.len() >= 5 {
                                data[4] as u32 * 5
                            } else {
                                0
                            };
                            info.hdmi_ver = Some(if max_tmds_mhz >= 600 {
                                "HDMI 2.1".into()
                            } else {
                                "HDMI 2.0".into()
                            });
                            if data.len() >= 9 && data[8] & 0x80 != 0 {
                                info.dsc = true;
                            }
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
    let home = std::env::var("HOME").map_err(|_| "HOME env var not set".to_string())?;
    let svc_dir = format!("{home}/.config/systemd/user");
    let svc_path = format!("{svc_dir}/kms-hdr-dim.service");

    if minutes == 0 {
        let _ = std::fs::remove_file(&svc_path);
        let s = tokio::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status()
            .await
            .map_err(|e| e.to_string())?;
        if !s.success() {
            return Err(format!("systemctl --user daemon-reload failed: {s}"));
        }
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

    tokio::fs::create_dir_all(&svc_dir)
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::write(&svc_path, content)
        .await
        .map_err(|e| e.to_string())?;
    let s = tokio::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if !s.success() {
        return Err(format!("systemctl --user daemon-reload failed: {s}"));
    }
    let s = tokio::process::Command::new("systemctl")
        .args(["--user", "enable", "--now", "kms-hdr-dim.service"])
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if !s.success() {
        return Err(format!(
            "systemctl --user enable --now kms-hdr-dim.service failed: {s}"
        ));
    }
    Ok(())
}

// ── Calibration patterns ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalibPattern {
    Black,
    DarkGray,
    Gray50,
    White,
    Red,
    Green,
    Blue,
    SdrHdrSplit,
}

impl CalibPattern {
    fn label(self) -> &'static str {
        match self {
            Self::Black => "Black",
            Self::DarkGray => "5% Gray",
            Self::Gray50 => "50% Gray",
            Self::White => "White",
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::SdrHdrSplit => "SDR│HDR",
        }
    }
    fn arg(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::DarkGray => "darkgray",
            Self::Gray50 => "gray50",
            Self::White => "white",
            Self::Red => "red",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::SdrHdrSplit => "sdr_hdr",
        }
    }
}

// ── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    Loaded {
        enabled: bool,
        display: Option<DisplayInfo>,
        gpu: &'static str,
        conf: HdrConf,
    },
    OledDimScrub(u32),
    AutoMode(bool),
    HdrToggle(bool),
    SdrNits(u32),
    PeakNits(u32),
    Gamut(u32),
    GamutMode(usize),
    Saturation(u32),
    MidtoneGamma(u32),
    BitDepth(usize),
    Apply,
    Reset,
    Applied(Result<(), String>),
    ShowCalPat(CalibPattern),
    CalibrateHdr,
    CloseCalPat,
    // OLED Care
    ForceOled(bool),
    OledPreset(bool),
    OledDimTimeout(u32),
    OledDimApplied(Result<(), String>),
}

// ── From impls ────────────────────────────────────────────────────────────────

impl From<Message> for crate::pages::Message {
    fn from(m: Message) -> Self {
        crate::pages::Message::DisplayHdr(m)
    }
}

impl From<Message> for crate::Message {
    fn from(m: Message) -> Self {
        crate::Message::PageMessage(m.into())
    }
}

// ── Page state ────────────────────────────────────────────────────────────────

pub struct Page {
    entity: page::Entity,
    pub conf: HdrConf,
    pub hdr_enabled: bool,
    pub display: Option<DisplayInfo>,
    pub display_is_oled: bool,
    pub gpu_vendor: &'static str,
    pub is_nvidia: bool,
    pub status: Option<String>,
    pub cal_child: Option<std::process::Child>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            entity: page::Entity::default(),
            conf: HdrConf::default(),
            hdr_enabled: false,
            display: None,
            display_is_oled: false,
            gpu_vendor: "unknown",
            is_nvidia: false,
            status: None,
            cal_child: None,
        }
    }
}

impl Page {
    pub fn update(&mut self, message: Message) -> Task<crate::app::Message> {
        match message {
            Message::Loaded {
                enabled,
                display,
                gpu,
                conf,
            } => {
                self.conf = conf;
                self.hdr_enabled = enabled;
                self.gpu_vendor = gpu;
                self.is_nvidia = gpu == "nvidia";
                let edid_oled = display.as_ref().map(|d| d.is_oled).unwrap_or(false);
                self.display_is_oled = edid_oled || self.conf.force_oled;
                self.display = display;
            }

            Message::AutoMode(on) => {
                self.conf.auto_mode = on;
                self.status = Some(if on {
                    "Automatic — HDR managed from display…".into()
                } else {
                    "Manual control".into()
                });
                return cosmic::task::future(async move {
                    let result = set_auto_mode(on).await;
                    crate::app::Message::from(Message::Applied(result))
                });
            }

            Message::HdrToggle(on) => {
                self.hdr_enabled = on;
                self.status = Some(if on {
                    "Enabling HDR…".into()
                } else {
                    "Resetting to SDR…".into()
                });
                let c = self.conf.clone();
                return cosmic::task::future(async move {
                    let result = if on {
                        write_conf_and_apply(c).await
                    } else {
                        do_reset().await
                    };
                    crate::app::Message::from(Message::Applied(result))
                });
            }

            Message::SdrNits(v) => {
                self.conf.sdr_nits = v;
            }
            Message::PeakNits(v) => {
                self.conf.peak_nits = v;
            }
            Message::Gamut(v) => {
                self.conf.gamut = v;
            }
            Message::Saturation(v) => {
                self.conf.saturation = v;
            }
            Message::MidtoneGamma(v) => {
                self.conf.midtone_gamma = v;
            }
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
                    crate::app::Message::from(Message::Applied(result))
                });
            }
            Message::Reset => {
                self.hdr_enabled = false;
                self.status = Some("Resetting…".into());
                return cosmic::task::future(async move {
                    let result = do_reset().await;
                    crate::app::Message::from(Message::Applied(result))
                });
            }
            Message::Applied(Ok(())) => {
                self.status = Some("Applied ✓".into());
            }
            Message::Applied(Err(e)) => {
                self.status = Some(format!("Error: {e}"));
            }

            Message::ShowCalPat(pat) => {
                if let Some(mut c) = self.cal_child.take() {
                    let _ = c.kill();
                }
                match std::process::Command::new("python3")
                    .args([HDR_CAL, pat.arg()])
                    .spawn()
                {
                    Ok(child) => {
                        self.cal_child = Some(child);
                    }
                    Err(e) => {
                        self.status = Some(format!("hdr-cal: {e}"));
                    }
                }
            }
            Message::CalibrateHdr => {
                if let Some(mut c) = self.cal_child.take() {
                    let _ = c.kill();
                }
                let c = self.conf.clone();
                match std::process::Command::new("python3")
                    .args([
                        HDR_CAL,
                        "--calibrate",
                        "--sdr-nits",
                        &c.sdr_nits.to_string(),
                        "--peak-nits",
                        &c.peak_nits.to_string(),
                        "--gamut",
                        &c.gamut.to_string(),
                        "--bpc",
                        &c.max_bpc.to_string(),
                        "--gamut-mode",
                        &c.gamut_mode,
                    ])
                    .spawn()
                {
                    Ok(child) => {
                        self.cal_child = Some(child);
                    }
                    Err(e) => {
                        self.status = Some(format!("hdr-cal: {e}"));
                    }
                }
            }
            Message::CloseCalPat => {
                if let Some(mut c) = self.cal_child.take() {
                    let _ = c.kill();
                }
            }

            // OLED Care
            Message::ForceOled(on) => {
                self.conf.force_oled = on;
                self.display_is_oled =
                    on || self.display.as_ref().map(|d| d.is_oled).unwrap_or(false);
                let c = self.conf.clone();
                return cosmic::task::future(async move {
                    let result = write_conf_and_apply(c).await;
                    crate::app::Message::from(Message::Applied(result))
                });
            }
            Message::OledPreset(on) => {
                self.conf.oled_preset = on;
                if on {
                    self.conf.sdr_nits = 150;
                    self.conf.peak_nits = 600;
                } else {
                    self.conf.sdr_nits = 203;
                    self.conf.peak_nits = 800;
                }
                self.status = Some("Applying OLED preset…".into());
                let c = self.conf.clone();
                return cosmic::task::future(async move {
                    let result = write_conf_and_apply(c).await;
                    crate::app::Message::from(Message::Applied(result))
                });
            }
            Message::OledDimScrub(v) => {
                self.conf.oled_dim_min = v;
            }
            Message::OledDimTimeout(minutes) => {
                self.conf.oled_dim_min = minutes;
                self.status = Some(if minutes == 0 {
                    "OLED auto-dim disabled".into()
                } else {
                    format!("Setting auto-dim to {minutes} min…")
                });
                return cosmic::task::future(async move {
                    let result = setup_oled_dim(minutes).await;
                    crate::app::Message::from(Message::OledDimApplied(result))
                });
            }
            Message::OledDimApplied(Ok(())) => {
                self.status = Some("OLED auto-dim updated ✓".into());
            }
            Message::OledDimApplied(Err(e)) => {
                self.status = Some(format!("OLED dim error: {e}"));
            }
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
                text::caption(if ok {
                    format!("{label} ✓")
                } else {
                    format!("{label} —")
                })
            };

            let hdr_row = row::with_capacity(4)
                .spacing(sp.space_xs)
                .push(cap("HDR10", d.hdr10))
                .push(cap("HLG", d.hlg))
                .push(cap("HDR10+", d.hdr10plus))
                .push(cap("Dolby Vision", d.dolby));

            let feat_row = row::with_capacity(5)
                .spacing(sp.space_xs)
                .push(cap("BT.2020", d.bt2020))
                .push(cap("DCI-P3", d.dci_p3))
                .push(cap("DSC", d.dsc))
                .push(cap("HDMI-CEC", d.cec))
                .push(cap("OLED", d.is_oled));

            let iface = d
                .hdmi_ver
                .as_deref()
                .or(d.dp_ver.as_deref())
                .unwrap_or("Unknown interface");
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
                                .push(feat_row),
                        ),
                ),
            );
        }

        // ── Automatic / Manual ────────────────────────────────────────────────
        col = col.push(
            list_column().add(
                settings::item::builder("Automatic")
                    .description(
                        "HDR turns itself on when an HDR display is connected, using the \
                         display's own EDID. Turn off for manual control and calibration.",
                    )
                    .control(toggler(self.conf.auto_mode).on_toggle(|v| msg(Message::AutoMode(v)))),
            ),
        );

        // In Automatic mode the daemon does everything — show a status line and
        // stop here. All the knobs and the calibration wizard live under Manual.
        if self.conf.auto_mode {
            let hdr_capable = self
                .display
                .as_ref()
                .map(|d| d.hdr10 || d.hlg || d.hdr10plus || d.dolby)
                .unwrap_or(false);
            let status = if self.hdr_enabled {
                "HDR is on — configured automatically from your display."
            } else if hdr_capable {
                "HDR-capable display detected — it engages automatically."
            } else {
                "No HDR display detected. Connect one and HDR turns on by itself."
            };
            col = col
                .push(
                    text::body(status)
                        .apply(widget::container)
                        .padding([sp.space_xs, sp.space_s]),
                )
                .push(
                    text::caption(
                        "Want to fine-tune or calibrate? Turn off Automatic to reveal manual controls.",
                    )
                    .apply(widget::container)
                    .padding([0, sp.space_s]),
                );
            return col.into();
        }

        // ── GPU badge ─────────────────────────────────────────────────────────
        {
            let gpu_desc = match self.gpu_vendor {
                "amd" => "AMD  ·  Full pipeline: DEGAMMA + CTM + GAMMA + saturation",
                "intel" => "Intel  ·  Full pipeline: DEGAMMA + CTM + GAMMA + saturation",
                "nvidia" => {
                    "NVIDIA  ·  Gamma-only on desktop  ·  Full HDR + gaming features via Gamescope (see below)"
                }
                _ => "GPU vendor unknown",
            };
            col = col.push(
                text::caption(gpu_desc)
                    .apply(widget::container)
                    .padding([sp.space_xxs, sp.space_xs]),
            );
        }

        // ── OLED Care ─────────────────────────────────────────────────────────
        {
            col = col.push(text::heading("OLED Care"));
            let mut oled_col = list_column().add(
                settings::item::builder("OLED Panel Override")
                    .description("Force OLED Care features on (use if EDID doesn't report OLED)")
                    .control(
                        toggler(self.conf.force_oled).on_toggle(|v| msg(Message::ForceOled(v))),
                    ),
            );

            if self.display_is_oled {
                let dim_label = if self.conf.oled_dim_min == 0 {
                    "Off".to_string()
                } else {
                    format!("{} min", self.conf.oled_dim_min)
                };

                oled_col = oled_col
                    .add(
                        settings::item::builder("Longevity Preset")
                            .description(
                                "SDR 150 nit / peak 600 nit — reduces panel stress in daily use",
                            )
                            .control(
                                toggler(self.conf.oled_preset)
                                    .on_toggle(|v| msg(Message::OledPreset(v))),
                            ),
                    )
                    .add(
                        settings::item::builder("Auto-dim after idle")
                            .description(
                                "Dims to 50 nit when no input detected — requires swayidle",
                            )
                            .control(
                                row::with_capacity(2)
                                    .spacing(sp.space_s)
                                    .align_y(Alignment::Center)
                                    .push(
                                        widget::slider(0..=60u32, self.conf.oled_dim_min, |v| {
                                            msg(Message::OledDimScrub(v))
                                        })
                                        .step(5u32)
                                        .width(Length::Fill)
                                        .on_release(msg(
                                            Message::OledDimTimeout(self.conf.oled_dim_min),
                                        )),
                                    )
                                    .push(
                                        text::body(dim_label)
                                            .apply(widget::container)
                                            .width(Length::Fixed(52.0)),
                                    ),
                            ),
                    )
                    .add(
                        settings::item::builder("Pixel shift")
                            .description("Configure in COSMIC Display compositor settings")
                            .control(widget::Space::new().width(Length::Shrink)),
                    );
            }

            col = col.push(oled_col);
        }

        // ── HDR toggle ────────────────────────────────────────────────────────
        col = col.push(
            list_column().add(
                settings::item::builder("Enable HDR10")
                    .description("BT.2020 wide-colour + PQ (ST 2084) tone mapping via KMS")
                    .control(toggler(self.hdr_enabled).on_toggle(|v| msg(Message::HdrToggle(v)))),
            ),
        );

        // ── Brightness ────────────────────────────────────────────────────────
        let sdr_pq = nits_to_pq_percent(self.conf.sdr_nits);
        let sdr_row = settings::item::builder("SDR White Point")
            .description(
                "Brightness of desktop and SDR content in HDR mode (ITU-R BT.2408: 203 nits)",
            )
            .control(
                row::with_capacity(2)
                    .spacing(sp.space_s)
                    .align_y(Alignment::Center)
                    .push(
                        widget::slider(80..=400u32, self.conf.sdr_nits, |v| {
                            msg(Message::SdrNits(v))
                        })
                        .width(Length::Fill),
                    )
                    .push(
                        text::body(format!("{} nits  ({:.1}% PQ)", self.conf.sdr_nits, sdr_pq))
                            .apply(widget::container)
                            .width(Length::Fixed(140.0)),
                    ),
            );

        let peak_pq = nits_to_pq_percent(self.conf.peak_nits);
        let peak_row = settings::item::builder("Display Peak Luminance")
            .description("Your display's maximum HDR nits — used for HDR10 metadata signaling")
            .control(
                row::with_capacity(2)
                    .spacing(sp.space_s)
                    .align_y(Alignment::Center)
                    .push(
                        widget::slider(400..=1600u32, self.conf.peak_nits, |v| {
                            msg(Message::PeakNits(v))
                        })
                        .step(10u32)
                        .width(Length::Fill),
                    )
                    .push(
                        text::body(format!(
                            "{} nits  ({:.1}% PQ)",
                            self.conf.peak_nits, peak_pq
                        ))
                        .apply(widget::container)
                        .width(Length::Fixed(140.0)),
                    ),
            );

        col = col.push(list_column().add(sdr_row).add(peak_row));

        // ── Colour gamut ──────────────────────────────────────────────────────
        let colour_heading = if self.is_nvidia {
            "Colour  (AMD / Intel only)"
        } else {
            "Colour"
        };
        col = col.push(text::heading(colour_heading));

        let gamut_opts = vec![
            "BT.2020  (full wide colour — UHDTV / DCI cinemas)".to_string(),
            "DCI-P3 D65  (Apple / cinema wide-gamut, D65 white)".to_string(),
            "sRGB  (no gamut expansion — tone map only)".to_string(),
        ];
        let gamut_sel = match self.conf.gamut_mode.as_str() {
            "dci-p3" => Some(1usize),
            "srgb" => Some(2usize),
            _ => Some(0usize),
        };

        let expansion_row = settings::item::builder("Gamut Expansion")
            .description(
                "0% = sRGB passthrough  ·  100% = full target gamut via 3×3 CTM (AMD/Intel only)",
            )
            .control(
                row::with_capacity(2)
                    .spacing(sp.space_s)
                    .align_y(Alignment::Center)
                    .push(
                        widget::slider(0..=100u32, self.conf.gamut, |v| msg(Message::Gamut(v)))
                            .width(Length::Fill),
                    )
                    .push(
                        text::body(format!("{}%", self.conf.gamut))
                            .apply(widget::container)
                            .width(Length::Fixed(48.0)),
                    ),
            );

        col = col.push(
            list_column()
                .add(
                    settings::item::builder("Target Colour Space")
                        .description("Colour space the CTM matrix maps sRGB primaries into")
                        .control(
                            widget::dropdown(gamut_opts, gamut_sel, |i| msg(Message::GamutMode(i)))
                                .width(Length::Fixed(290.0)),
                        ),
                )
                .add(expansion_row),
        );

        // ── Color intensity ───────────────────────────────────────────────────
        let intensity_heading = if self.is_nvidia {
            "Color Intensity  (AMD / Intel only)"
        } else {
            "Color Intensity"
        };
        col = col.push(text::heading(intensity_heading));

        let sat_row = settings::item::builder("Saturation")
            .description(
                "Boost or reduce colour vividness · 100% = neutral · applied via BT.709 CTM",
            )
            .control(
                row::with_capacity(2)
                    .spacing(sp.space_s)
                    .align_y(Alignment::Center)
                    .push(
                        widget::slider(50..=200u32, self.conf.saturation, |v| {
                            msg(Message::Saturation(v))
                        })
                        .step(5u32)
                        .width(Length::Fill),
                    )
                    .push(
                        text::body(format!("{}%", self.conf.saturation))
                            .apply(widget::container)
                            .width(Length::Fixed(52.0)),
                    ),
            );
        col = col.push(list_column().add(sat_row));

        // ── Tone mapping ──────────────────────────────────────────────────────
        let mg_desc = match self.conf.midtone_gamma {
            v if v > 110 => format!("{}%  — HDR punch (darkened midtones, higher contrast)", v),
            v if v < 90 => format!("{}%  — lifted midtones (lower contrast)", v),
            v => format!("{}%  — neutral", v),
        };
        col = col.push(text::heading("Tone Mapping"));
        col = col.push(
            list_column().add(
                settings::item::builder("Midtone Gamma")
                    .description(mg_desc)
                    .control(
                        row::with_capacity(2)
                            .spacing(sp.space_s)
                            .align_y(Alignment::Center)
                            .push(
                                widget::slider(30..=250u32, self.conf.midtone_gamma, |v| {
                                    msg(Message::MidtoneGamma(v))
                                })
                                .step(5u32)
                                .width(Length::Fill),
                            )
                            .push(
                                text::body(format!("{}%", self.conf.midtone_gamma))
                                    .apply(widget::container)
                                    .width(Length::Fixed(52.0)),
                            ),
                    ),
            ),
        );

        // ── Output format ─────────────────────────────────────────────────────
        let bpc_opts = vec![
            "8 bpc  (legacy / compatibility)".to_string(),
            "10 bpc  (HDR10 — recommended)".to_string(),
            "12 bpc  (HDR+ / reference monitors)".to_string(),
        ];
        let bpc_sel = match self.conf.max_bpc {
            8 => Some(0usize),
            12 => Some(2),
            _ => Some(1),
        };

        col = col.push(
            list_column().add(
                settings::item::builder("Output Bit Depth")
                    .description("Requested via max_requested_bpc connector property")
                    .control(
                        widget::dropdown(bpc_opts, bpc_sel, |i| msg(Message::BitDepth(i)))
                            .width(Length::Fixed(290.0)),
                    ),
            ),
        );

        // Gaming / Gamescope / NVIDIA features now live on the dedicated
        // Settings → Displays → Gaming page (no longer duplicated here).
        if self.is_nvidia {
            col = col.push(
                text::caption(
                    "Gamescope, upscaling and NVIDIA gaming features are in Displays → Gaming.",
                )
                .apply(widget::container)
                .padding([sp.space_xxs, sp.space_xs]),
            );
        }

        // ── Calibration ───────────────────────────────────────────────────────
        const PATTERNS: &[CalibPattern] = &[
            CalibPattern::Black,
            CalibPattern::DarkGray,
            CalibPattern::Gray50,
            CalibPattern::White,
            CalibPattern::Red,
            CalibPattern::Green,
            CalibPattern::Blue,
            CalibPattern::SdrHdrSplit,
        ];

        let mut pat_row = row::with_capacity(10)
            .spacing(sp.space_xxs)
            .align_y(Alignment::Center);
        for &p in PATTERNS {
            pat_row = pat_row
                .push(widget::button::standard(p.label()).on_press(msg(Message::ShowCalPat(p))));
        }
        if self.cal_child.is_some() {
            pat_row = pat_row
                .push(widget::button::destructive("✕ Close").on_press(msg(Message::CloseCalPat)));
        }

        col = col.push(
            list_column()
                .add(
                    settings::item::builder("Calibrate HDR")
                        .description(
                            "Adjust SDR white brightness interactively against a reference pattern",
                        )
                        .control(
                            widget::button::suggested("Calibrate…")
                                .on_press(msg(Message::CalibrateHdr)),
                        ),
                )
                .add(
                    settings::item::builder("Test Patterns")
                        .description("Full-screen colour fields — press Esc or click to close")
                        .control(pat_row),
                ),
        );

        // ── Status + action row ───────────────────────────────────────────────
        let mut action_row = row::with_capacity(3)
            .spacing(sp.space_s)
            .align_y(Alignment::Center)
            .padding([0, 0, sp.space_s, 0]);

        if let Some(ref s) = self.status {
            action_row = action_row.push(
                text::caption(s.as_str())
                    .apply(widget::container)
                    .width(Length::Fill),
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
    crate::pages::Message::DisplayHdr(m)
}

// ── page::Page impl ───────────────────────────────────────────────────────────

impl page::AutoBind<crate::pages::Message> for Page {}

impl page::Page<crate::pages::Message> for Page {
    fn info(&self) -> page::Info {
        page::Info::new("display-hdr", "video-display-symbolic")
            .title("HDR & Wide Colour")
            .description("Tone mapping, colour gamut, calibration and gaming settings")
    }

    fn set_id(&mut self, entity: page::Entity) {
        self.entity = entity;
    }

    fn content(
        &self,
        sections: &mut slotmap::SlotMap<section::Entity, Section<crate::pages::Message>>,
    ) -> Option<page::Content> {
        Some(vec![sections.insert(hdr_view_section())])
    }

    fn on_enter(&mut self) -> Task<crate::pages::Message> {
        cosmic::task::future(async move {
            let (enabled, display, gpu, conf) = tokio::task::spawn_blocking(|| {
                (
                    service_active(),
                    parse_edid_blocking(),
                    gpu_vendor(),
                    read_conf(),
                )
            })
            .await
            .unwrap_or_else(|e| {
                tracing::error!("HDR on_enter task panicked: {e:?}");
                (false, None, "unknown", HdrConf::default())
            });
            crate::pages::Message::DisplayHdr(Message::Loaded {
                enabled,
                display,
                gpu,
                conf,
            })
        })
    }

    fn on_leave(&mut self) -> Task<crate::pages::Message> {
        if let Some(mut c) = self.cal_child.take() {
            let _ = c.kill();
        }
        Task::none()
    }
}

fn hdr_view_section() -> Section<crate::pages::Message> {
    Section::default()
        .title("HDR")
        .search_ignore()
        .view::<Page>(|_binder, page, _section| page.view())
}
