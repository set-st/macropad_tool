use eframe::egui;
use crate::options::{Options, Command, DevelOptions};
use crate::consts::VENDOR_ID;
use crate::mapping::{Mapping, Macropad, Layer, LedSettings};
use crate::keyboard::LedColor;
use crate::config::Orientation;
use crate::{open_keyboard, find_device};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use std::thread;

pub fn main() {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 900.0])
            .with_min_inner_size([850.0, 700.0]),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Macropad Editor Pro",
        native_options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(MacropadApp::new()))
        }),
    );
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Selection {
    None,
    Button(usize, usize), 
    Knob(usize, KnobPart),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum KnobPart { CCW, Press, CW }

struct EditorData {
    current_layer_idx: usize,
    macropad_data: Macropad,
    selection: Selection,
    connected_pid: Option<u16>,
    status_msg: String,
    status_color: egui::Color32,
}

lazy_static::lazy_static! {
    static ref DATA: Arc<Mutex<EditorData>> = Arc::new(Mutex::new(EditorData {
        current_layer_idx: 0,
        macropad_data: Macropad::new(2, 3, 1),
        selection: Selection::None,
        connected_pid: None,
        status_msg: "Welcome to Macropad Editor Pro".to_string(),
        status_color: egui::Color32::LIGHT_GRAY,
    }));
}

struct MacropadApp {
    last_conn_check: Instant,
    temp_editor_val: String,
    temp_delay_val: String,
    
    ui_rows: u8,
    ui_cols: u8,
    ui_knobs: u8,
    ui_layers: u8,
    ui_orientation: Orientation,

    led_mode: u8,
    led_layer: u8,
    led_color: LedColor,
}

impl MacropadApp {
    fn new() -> Self {
        let initial_data = Mapping::read("mapping.ron").unwrap_or_else(|_| Macropad::new(2, 3, 1));
        
        let (led_m, led_l, led_c) = if let Some(led) = &initial_data.led_settings {
            (led.mode, led.layer, led.color)
        } else {
            (1, 1, LedColor::Cyan)
        };

        let initial_rows = initial_data.device.rows;
        let initial_cols = initial_data.device.cols;
        let initial_knobs = initial_data.device.knobs;
        let initial_layers = initial_data.device.layers;
        let initial_orient = initial_data.device.orientation;

        {
            let mut d = DATA.lock().unwrap();
            d.macropad_data = initial_data;
        }
        
        Self {
            last_conn_check: Instant::now() - Duration::from_secs(10),
            temp_editor_val: String::new(),
            temp_delay_val: String::new(),
            ui_rows: initial_rows,
            ui_cols: initial_cols,
            ui_knobs: initial_knobs,
            ui_layers: initial_layers,
            ui_orientation: initial_orient,
            led_mode: led_m,
            led_layer: led_l,
            led_color: led_c,
        }
    }

    fn check_connection() {
        thread::spawn(|| {
            let pid = match find_device(VENDOR_ID, None) {
                Ok((_, _, pid)) => Some(pid),
                Err(_) => None,
            };
            if let Ok(mut data) = DATA.lock() {
                data.connected_pid = pid;
            }
        });
    }

    fn set_status(msg: &str, color: egui::Color32) {
        if let Ok(mut data) = DATA.lock() {
            data.status_msg = msg.to_string();
            data.status_color = color;
        }
    }

    fn apply_layout(&mut self) {
        let mut data = DATA.lock().unwrap();
        data.macropad_data.device.rows = self.ui_rows;
        data.macropad_data.device.cols = self.ui_cols;
        data.macropad_data.device.knobs = self.ui_knobs;
        data.macropad_data.device.layers = self.ui_layers;
        data.macropad_data.device.orientation = self.ui_orientation;
        
        let old_layers = data.macropad_data.layers.clone();
        data.macropad_data.layers = vec![Layer::new(self.ui_rows, self.ui_cols, self.ui_knobs); self.ui_layers as usize];
        
        for i in 0..(self.ui_layers as usize) {
            if i < old_layers.len() {
                let mut new_layer = Layer::new(self.ui_rows, self.ui_cols, self.ui_knobs);
                for row in 0..(self.ui_rows as usize) {
                    for col in 0..(self.ui_cols as usize) {
                        if row < old_layers[i].buttons.len() && col < old_layers[i].buttons[row].len() {
                            new_layer.buttons[row][col] = old_layers[i].buttons[row][col].clone();
                        }
                    }
                }
                for knob in 0..(self.ui_knobs as usize) {
                    if knob < old_layers[i].knobs.len() {
                        new_layer.knobs[knob] = old_layers[i].knobs[knob].clone();
                    }
                }
                data.macropad_data.layers[i] = new_layer;
            }
        }
        
        data.selection = Selection::None;
        if data.current_layer_idx >= self.ui_layers as usize { data.current_layer_idx = 0; }
        self.temp_editor_val = String::new();
        self.temp_delay_val = String::new();
        data.status_msg = format!("Applied: {} layers, {}x{} grid.", self.ui_layers, self.ui_rows, self.ui_cols);
        data.status_color = egui::Color32::KHAKI;
    }

    fn sync_temp_to_data(&self, data: &mut MutexGuard<EditorData>) {
        let layer_idx = data.current_layer_idx;
        let delay = self.temp_delay_val.parse::<u16>().unwrap_or(0);
        match data.selection {
            Selection::Button(r, c) => {
                if layer_idx < data.macropad_data.layers.len() {
                    data.macropad_data.layers[layer_idx].buttons[r][c].mapping = self.temp_editor_val.clone();
                    data.macropad_data.layers[layer_idx].buttons[r][c].delay = delay;
                }
            }
            Selection::Knob(idx, part) => {
                if layer_idx < data.macropad_data.layers.len() {
                    let knob = &mut data.macropad_data.layers[layer_idx].knobs[idx];
                    match part {
                        KnobPart::CCW => { knob.ccw.mapping = self.temp_editor_val.clone(); knob.ccw.delay = delay; }
                        KnobPart::Press => { knob.press.mapping = self.temp_editor_val.clone(); knob.press.delay = delay; }
                        KnobPart::CW => { knob.cw.mapping = self.temp_editor_val.clone(); knob.cw.delay = delay; }
                    }
                }
            }
            Selection::None => {}
        }
        data.macropad_data.led_settings = Some(LedSettings { mode: self.led_mode, layer: self.led_layer, color: self.led_color });
    }

    fn sync_data_to_temp(&mut self, data: &EditorData) {
        let layer_idx = data.current_layer_idx;
        if layer_idx >= data.macropad_data.layers.len() { return; }
        match data.selection {
            Selection::Button(r, c) => {
                let btn = &data.macropad_data.layers[layer_idx].buttons[r][c];
                self.temp_editor_val = btn.mapping.clone();
                self.temp_delay_val = btn.delay.to_string();
            }
            Selection::Knob(idx, part) => {
                let btn = match part {
                    KnobPart::CCW => &data.macropad_data.layers[layer_idx].knobs[idx].ccw,
                    KnobPart::Press => &data.macropad_data.layers[layer_idx].knobs[idx].press,
                    KnobPart::CW => &data.macropad_data.layers[layer_idx].knobs[idx].cw,
                };
                self.temp_editor_val = btn.mapping.clone();
                self.temp_delay_val = btn.delay.to_string();
            }
            Selection::None => { self.temp_editor_val = String::new(); self.temp_delay_val = String::new(); }
        }
    }

    fn get_led_modes(pid: u16) -> Vec<(u8, &'static str)> {
        if pid == 0x8890 { vec![ (0, "Off"), (1, "Last Pushed"), (2, "Cycle Colors") ] }
        else { vec![ (0, "Off"), (1, "Always On (Color)"), (2, "Shock (Color)"), (3, "Shock2 (Color)"), (4, "Light Key (Color)"), (5, "White Always On") ] }
    }
}

impl eframe::App for MacropadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.last_conn_check.elapsed() > Duration::from_secs(2) { Self::check_connection(); self.last_conn_check = Instant::now(); }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            let data = DATA.lock().unwrap();
            ui.horizontal(|ui| {
                ui.heading("⌨ Macropad Editor Pro");
                ui.separator();
                if let Some(pid) = data.connected_pid {
                    ui.label(egui::RichText::new(format!("CONNECTED (0x{:04x}) ✅", pid)).color(egui::Color32::GREEN));
                    ui.separator();
                    let hint = if pid == 0x8890 { "ℹ Single-layer device detected." } else { "ℹ Multi-layer device detected." };
                    ui.label(egui::RichText::new(hint).italics().size(12.0).color(egui::Color32::LIGHT_BLUE));
                } else { ui.label(egui::RichText::new("DISCONNECTED ❌").color(egui::Color32::RED)); }
            });
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            let data = DATA.lock().unwrap();
            ui.horizontal(|ui| { ui.label(egui::RichText::new(&data.status_msg).color(data.status_color)); });
        });

        egui::SidePanel::left("side_panel").width_range(200.0..=250.0).show(ctx, |ui| {
            let (rows, cols, knobs, layers, orientation, pid) = {
                let d = DATA.lock().unwrap();
                (d.macropad_data.device.rows, d.macropad_data.device.cols, d.macropad_data.device.knobs, d.macropad_data.device.layers, d.macropad_data.device.orientation, d.connected_pid.unwrap_or(0x8840))
            };

            ui.heading("Device Config");
            ui.add_space(8.0);
            
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Layers:").strong());
                egui::ComboBox::from_id_salt("layers_cb").selected_text(self.ui_layers.to_string()).show_ui(ui, |ui| {
                    for i in 1..=3 { ui.selectable_value(&mut self.ui_layers, i, i.to_string()); }
                });
            });
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("Rows:");
                egui::ComboBox::from_id_salt("rows_cb").selected_text(self.ui_rows.to_string()).show_ui(ui, |ui| {
                    for i in 1..=10 { ui.selectable_value(&mut self.ui_rows, i, i.to_string()); }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Cols:");
                egui::ComboBox::from_id_salt("cols_cb").selected_text(self.ui_cols.to_string()).show_ui(ui, |ui| {
                    for i in 1..=10 { ui.selectable_value(&mut self.ui_cols, i, i.to_string()); }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Knobs:");
                egui::ComboBox::from_id_salt("knobs_cb").selected_text(self.ui_knobs.to_string()).show_ui(ui, |ui| {
                    for i in 0..=10 { ui.selectable_value(&mut self.ui_knobs, i, i.to_string()); }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Orientation:");
                egui::ComboBox::from_id_salt("orient_cb").selected_text(format!("{:?}", self.ui_orientation)).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.ui_orientation, Orientation::Normal, "Normal");
                    ui.selectable_value(&mut self.ui_orientation, Orientation::Clockwise, "Clockwise");
                    ui.selectable_value(&mut self.ui_orientation, Orientation::CounterClockwise, "CounterClockwise");
                    ui.selectable_value(&mut self.ui_orientation, Orientation::UpsideDown, "UpsideDown");
                });
            });
            
            let changed = self.ui_rows != rows || self.ui_cols != cols || self.ui_knobs != knobs || self.ui_layers != layers || self.ui_orientation != orientation;
            if changed {
                ui.add_space(10.0);
                if ui.button(egui::RichText::new("Apply Layout").color(egui::Color32::GOLD)).clicked() { self.apply_layout(); }
            }
            
            ui.add_space(20.0); ui.separator(); ui.add_space(10.0);
            ui.heading("LED Control");
            let modes = Self::get_led_modes(pid);
            ui.horizontal(|ui| {
                ui.label("Mode:");
                let current_mode_text = modes.iter().find(|(m, _)| *m == self.led_mode).map(|(_, t)| *t).unwrap_or("Unknown");
                egui::ComboBox::from_id_salt("led_mode_cb").selected_text(current_mode_text).show_ui(ui, |ui| {
                    for (m, t) in modes { ui.selectable_value(&mut self.led_mode, m, t); }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Layer:");
                egui::ComboBox::from_id_salt("led_layer_cb").selected_text(format!("Layer {}", self.led_layer)).show_ui(ui, |ui| {
                    for i in 1..=3 { ui.selectable_value(&mut self.led_layer, i, format!("Layer {}", i)); }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Color:");
                egui::ComboBox::from_id_salt("led_color_cb").selected_text(format!("{:?}", self.led_color)).show_ui(ui, |ui| {
                    for color in [LedColor::Red, LedColor::Orange, LedColor::Yellow, LedColor::Green, LedColor::Cyan, LedColor::Blue, LedColor::Purple] {
                        ui.selectable_value(&mut self.led_color, color, format!("{:?}", color));
                    }
                });
            });
            if pid == 0x8890 { ui.label(egui::RichText::new("Note: Color might not work on 8890").italics().size(10.0).color(egui::Color32::KHAKI)); }

            if ui.button("Apply LED").clicked() {
                let mut d = DATA.lock().unwrap(); self.sync_temp_to_data(&mut d); let _ = Mapping::save(&d.macropad_data, "mapping.ron");
                let mode = self.led_mode; let color = self.led_color; let layer = self.led_layer;
                thread::spawn(move || {
                    let options = Options { command: Command::ShowGui, devel_options: DevelOptions { vendor_id: VENDOR_ID, product_id: None, address: None, out_endpoint_address: None, in_endpoint_address: None, interface_number: None } };
                    match open_keyboard(&options) {
                        Ok(mut kb) => { if let Err(e) = kb.set_led(mode, layer, color) { Self::set_status(&format!("❌ LED Error: {}", e), egui::Color32::RED); } else { Self::set_status("✅ LED updated!", egui::Color32::GREEN); } }
                        Err(e) => Self::set_status(&format!("❌ USB error: {}", e), egui::Color32::RED),
                    }
                });
            }

            ui.add_space(20.0); ui.separator(); ui.add_space(20.0);
            if ui.add_sized([ui.available_width(), 40.0], egui::Button::new("💾 Save Config")).clicked() {
                let mut d = DATA.lock().unwrap(); self.sync_temp_to_data(&mut d); let _ = Mapping::save(&d.macropad_data, "mapping.ron");
                d.status_msg = format!("✅ Config saved to mapping.ron"); d.status_color = egui::Color32::GREEN;
            }
            ui.add_space(10.0);
            if ui.add_sized([ui.available_width(), 40.0], egui::Button::new("🚀 Program Device").fill(egui::Color32::from_rgb(0, 80, 0))).clicked() {
                let mut d = DATA.lock().unwrap(); self.sync_temp_to_data(&mut d); let config = d.macropad_data.clone();
                d.status_msg = "🚀 Programming...".to_string(); d.status_color = egui::Color32::GOLD;
                thread::spawn(move || {
                    let options = Options { command: Command::ShowGui, devel_options: DevelOptions { vendor_id: VENDOR_ID, product_id: None, address: None, out_endpoint_address: None, in_endpoint_address: None, interface_number: None } };
                    match open_keyboard(&options) {
                        Ok(mut kb) => { match kb.program(&config) { Ok(_) => Self::set_status("✅ Programmed successfully!", egui::Color32::GREEN), Err(e) => Self::set_status(&format!("❌ Error: {}", e), egui::Color32::RED) } }
                        Err(e) => Self::set_status(&format!("❌ USB error: {}", e), egui::Color32::RED),
                    }
                });
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                let mut d = DATA.lock().unwrap();
                let num_layers = d.macropad_data.device.layers as usize;
                for i in 0..num_layers { if ui.selectable_label(d.current_layer_idx == i, format!("Layer {}", i + 1)).clicked() { self.sync_temp_to_data(&mut d); d.current_layer_idx = i; self.sync_data_to_temp(&d); } }
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut d = DATA.lock().unwrap();
                let layer_idx = d.current_layer_idx;
                if layer_idx >= d.macropad_data.layers.len() { return; }
                ui.heading(format!("Layer {} Matrix", layer_idx + 1));
                let r = d.macropad_data.device.rows as usize;
                let c = d.macropad_data.device.cols as usize;
                let k = d.macropad_data.device.knobs as usize;

                egui::Grid::new("grid").spacing([10.0, 10.0]).show(ui, |ui| {
                    for row in 0..r {
                        for col in 0..c {
                            let val = &d.macropad_data.layers[layer_idx].buttons[row][col].mapping;
                            let is_selected = d.selection == Selection::Button(row, col);
                            let btn_text = if val.is_empty() { format!("[{},{}]", row+1, col+1) } else { val.clone() };
                            if ui.add_sized([100.0, 40.0], egui::Button::new(btn_text).selected(is_selected)).clicked() { self.sync_temp_to_data(&mut d); d.selection = Selection::Button(row, col); self.sync_data_to_temp(&d); }
                        }
                        ui.end_row();
                    }
                });

                if k > 0 {
                    ui.add_space(20.0); ui.heading("Rotary Encoders");
                    for i in 0..k {
                        ui.horizontal(|ui| {
                            ui.label(format!("Knob {}:", i+1));
                            for (part, label) in [(KnobPart::CCW, "CCW"), (KnobPart::Press, "Press"), (KnobPart::CW, "CW")] {
                                let val = match part { KnobPart::CCW => &d.macropad_data.layers[layer_idx].knobs[i].ccw.mapping, KnobPart::Press => &d.macropad_data.layers[layer_idx].knobs[i].press.mapping, KnobPart::CW => &d.macropad_data.layers[layer_idx].knobs[i].cw.mapping };
                                let is_selected = d.selection == Selection::Knob(i, part);
                                let btn_text = if val.is_empty() { label } else { val };
                                if ui.add(egui::Button::new(btn_text).selected(is_selected)).clicked() { self.sync_temp_to_data(&mut d); d.selection = Selection::Knob(i, part); self.sync_data_to_temp(&d); }
                            }
                        });
                    }
                }

                ui.add_space(20.0); ui.separator();
                if d.selection != Selection::None {
                    ui.heading("Edit Selection");
                    ui.horizontal(|ui| {
                        ui.label("Delay (ms):"); if ui.text_edit_singleline(&mut self.temp_delay_val).changed() { self.sync_temp_to_data(&mut d); }
                        ui.add_space(20.0); ui.label("Mapping:"); if ui.text_edit_singleline(&mut self.temp_editor_val).changed() { self.sync_temp_to_data(&mut d); }
                    });
                    ui.add_space(10.0);
                    ui.heading("Code Reference Legend");
                    ui.group(|ui| {
                        ui.horizontal(|ui| { ui.label(egui::RichText::new("Modifiers:").strong()); ui.label("ctrl-, shift-, alt-, win-, rctrl-, rshift-, ralt-, rwin-"); });
                        ui.horizontal(|ui| { ui.label(egui::RichText::new("Media:").strong()); ui.label("play, stop, next, prev, mute, volup, voldown, brightnessup, brightnessdown"); });
                        ui.horizontal(|ui| { ui.label(egui::RichText::new("Mouse:").strong()); ui.label("click, rclick, mclick, wheelup, wheeldown"); });
                        ui.horizontal(|ui| { ui.label(egui::RichText::new("Other:").strong()); ui.label("space, enter, backspace, tab, esc, comma, dot, slash, a-z, 0-9, f1-f24"); });
                        ui.label(egui::RichText::new("Hint: Use commas to sequence commands (e.g. ctrl-c,ctrl-v) and dashes for combos (e.g. shift-a)").italics().size(11.0));
                    });
                } else { ui.label(egui::RichText::new("Click a button in the grid above to edit its configuration").italics()); }
            });
        });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}
