use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use crate::keyboard::{LedColor, MediaCode, Modifier, WellKnownCode};
use crate::config::Orientation;
use crate::consts;

/// Mapping for a button
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Button {
    pub delay: u16,
    pub mapping: String,
}

impl Button {
    pub fn new() -> Self {
        Self { delay: 0, mapping: String::new() }
    }
}

/// Mapping for a knob
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Knob {
    pub ccw: Button,
    pub press: Button,
    pub cw: Button,
}

/// Layer configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Layer {
    pub buttons: Vec<Vec<Button>>,
    pub knobs: Vec<Knob>,
}

impl Layer {
    pub fn new(rows: u8, cols: u8, num_knobs: u8) -> Self {
        let mut buttons = Vec::new();
        for _ in 0..rows { buttons.push(vec![Button::new(); cols.into()]); }
        let mut knobs = Vec::new();
        for _ in 0..num_knobs { knobs.push(Knob { ccw: Button::new(), press: Button::new(), cw: Button::new() }); }
        Self { buttons, knobs }
    }
}

fn default_layers_count() -> u8 { 3 }

/// Device configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Device {
    pub orientation: Orientation,
    pub rows: u8,
    pub cols: u8,
    pub knobs: u8,
    #[serde(default = "default_layers_count")]
    pub layers: u8,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct LedSettings {
    pub mode: u8,
    pub layer: u8,
    pub color: LedColor,
}

/// Mapping configuration of a macropad
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Macropad {
    pub device: Device,
    pub layers: Vec<Layer>,
    pub led_settings: Option<LedSettings>,
}

impl Macropad {
    pub fn new(rows: u8, cols: u8, knobs: u8) -> Self {
        let layers_count = 3;
        Self {
            device: Device { orientation: Orientation::Normal, rows, cols, knobs, layers: layers_count },
            layers: vec![Layer::new(rows, cols, knobs); layers_count as usize],
            led_settings: Some(LedSettings { mode: 1, layer: 1, color: LedColor::Cyan }),
        }
    }
}

use ron::de::from_reader;
use ron::ser::{to_string_pretty, PrettyConfig};
use std::fs::File;
use std::str::FromStr;

pub struct Mapping {}

impl Mapping {
    pub fn config_path() -> std::path::PathBuf {
        let mut path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
        path.pop();
        path.push("mapping.ron");
        path
    }

    pub fn read(cfg_file: &str) -> Result<Macropad> {
        let path = if cfg_file == "mapping.ron" { Self::config_path() } else { std::path::PathBuf::from(cfg_file) };
        if !path.exists() {
            let default_config = Macropad::new(2, 3, 1);
            Self::save(&default_config, path.to_str().unwrap()).context("Creating default config")?;
        }
        let f = File::open(path).context("Failed opening file")?;
        let config: Macropad = from_reader(f).map_err(|e| anyhow!("Failed to load config: {e}"))?;
        Ok(config)
    }

    pub fn print(config: Macropad) {
        let pretty = PrettyConfig::new().depth_limit(4).separate_tuple_members(true).enumerate_arrays(false);
        let s = to_string_pretty(&config, pretty).expect("Serialization failed");
        println!("{s}");
    }

    pub fn save(config: &Macropad, cfg_file: &str) -> Result<()> {
        let path = if cfg_file == "mapping.ron" { Self::config_path() } else { std::path::PathBuf::from(cfg_file) };
        let pretty = PrettyConfig::new().depth_limit(4).separate_tuple_members(true).enumerate_arrays(false);
        let s = to_string_pretty(config, pretty).map_err(|e| anyhow!("Serialization failed: {}", e))?;
        std::fs::write(path, s).map_err(|e| anyhow!("Failed to write file: {}", e))?;
        Ok(())
    }

    pub fn validate(cfg_file: &str, pid: Option<u16>) -> Result<()> {
        let mut max_programmable_keys = 0xff;
        if let Some(max) = pid {
            match max {
                0x8840 | 0x8842 => max_programmable_keys = consts::MAX_KEY_PRESSES_884X,
                0x8890 => max_programmable_keys = consts::MAX_KEY_PRESSES_8890,
                _ => return Err(anyhow!("Unknown product id 0x{:02x}", max)),
            }
        }
        let cfg = Self::read(cfg_file)?;
        if cfg.layers.is_empty() || cfg.layers.len() > 3 { return Err(anyhow!("number of layers must be > 0 and < 4")); }
        for (i, layer) in cfg.layers.iter().enumerate() {
            if layer.buttons.len() != cfg.device.rows.into() { return Err(anyhow!("rows mismatch at layer {}", i+1)); }
            for (j, btn_mapping) in layer.buttons.iter().enumerate() {
                if btn_mapping.len() != cfg.device.cols.into() { return Err(anyhow!("cols mismatch at layer {} row {}", i+1, j+1)); }
                for (k, btn) in btn_mapping.iter().enumerate() {
                    Self::validate_key_mapping(btn, max_programmable_keys, pid).context(format!("layer {} row {} btn {}", i+1, j+1, k+1))?;
                }
            }
            if layer.knobs.len() != cfg.device.knobs.into() { return Err(anyhow!("knobs mismatch at layer {}", i+1)); }
            for (k, knob) in layer.knobs.iter().enumerate() {
                Self::validate_key_mapping(&knob.ccw, max_programmable_keys, pid).context(format!("layer {} knob {} ccw", i+1, k+1))?;
                Self::validate_key_mapping(&knob.press, max_programmable_keys, pid).context(format!("layer {} knob {} press", i+1, k+1))?;
                Self::validate_key_mapping(&knob.cw, max_programmable_keys, pid).context(format!("layer {} knob {} cw", i+1, k+1))?;
            }
        }
        Ok(())
    }

    fn validate_key_mapping(btn: &Button, max_size: usize, pid: Option<u16>) -> Result<()> {
        let keys: Vec<_> = btn.mapping.split(',').collect();
        if keys.len() > max_size { return Err(anyhow!("Too many keys")); }
        if max_size == consts::MAX_KEY_PRESSES_8890 {
            if btn.delay > 0 { println!("Warning - 0x8890 doesn't support delay"); }
        } else if btn.delay > consts::MAX_DELAY { return Err(anyhow!("delay too high")); }
        for (i, k) in keys.iter().enumerate() {
            let single_key: Vec<_> = k.split('-').collect();
            if max_size == consts::MAX_KEY_PRESSES_8890 && i > 0 && single_key.len() > 1 { return Err(anyhow!("0x8890 only supports mods on first key")); }
            for sk in single_key {
                let da_key = Self::uppercase_first(sk);
                let mut found = false;
                if Self::is_modifier_key(&da_key) { found = true; }
                else if Self::is_media_key(&da_key) {
                    found = true;
                    if pid == Some(0x8890) {
                        match da_key.as_str() { "Play" | "Previous" | "Next" | "Mute" | "Volumeup" | "Volumedown" => (), _ => return Err(anyhow!("unsupported media key for 8890")), }
                    }
                }
                else if Self::is_regular_key(&da_key) { found = true; }
                else if Self::is_mouse_action(&da_key) { found = true; }
                if !found { return Err(anyhow!("unknown key - {}", sk)); }
            }
        }
        Ok(())
    }

    fn uppercase_first(data: &str) -> String {
        let mut result = String::new();
        let mut first = true;
        for value in data.chars() {
            if first { result.push(value.to_ascii_uppercase()); first = false; }
            else { result.push(value); }
        }
        result
    }

    fn is_modifier_key(keystr: &str) -> bool { Modifier::from_str(keystr).is_ok() }
    fn is_media_key(keystr: &str) -> bool { MediaCode::from_str(keystr).is_ok() }
    fn is_regular_key(keystr: &str) -> bool { WellKnownCode::from_str(keystr).is_ok() }
    fn is_mouse_action(keystr: &str) -> bool { matches!(keystr.to_lowercase().as_str(), "wheelup" | "wheeldown" | "click" | "mclick" | "rclick") }
}
