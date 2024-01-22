use input::event::keyboard::{KeyState, KeyboardEventTrait};
use input::event::{EventTrait, PointerEvent};
use input::{Device, Event, Libinput, LibinputInterface};
use libc::{O_RDONLY, O_RDWR, O_WRONLY};
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::os::unix::{fs::OpenOptionsExt, io::OwnedFd};
use std::path::Path;

struct Interface;

#[allow(clippy::bad_bit_mask)]
impl LibinputInterface for Interface {
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        dbg!(path);
        OpenOptions::new()
            .custom_flags(flags)
            .read((flags & O_RDONLY != 0) | (flags & O_RDWR != 0))
            .write((flags & O_WRONLY != 0) | (flags & O_RDWR != 0))
            .open(path)
            .map(|file| file.into())
            .map_err(|err| err.raw_os_error().unwrap())
    }
    fn close_restricted(&mut self, fd: OwnedFd) {
        dbg!();
        drop(File::from(fd));
    }
}

use eframe::egui;
use egui::{Button, DragValue, Ui, Widget};

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|cc| Box::new(MyEguiApp::new(cc))),
    )
    .unwrap();
}

#[derive(Eq, PartialEq)]
enum KeyBindState {
    Unbound,
    Binding,
    Bound(u32, bool),
}

impl KeyBindState {
    fn poll_triggered(&mut self) -> bool {
        match self {
            KeyBindState::Bound(_, x) => {
                let v = *x;
                *x = false;
                v
            }
            _ => false,
        }
    }
}

#[derive(Eq, PartialEq)]
enum AppTab {
    Basic,
    Record,
}

struct MyEguiApp {
    mouse_states: HashSet<Device>,
    active_mouse: Option<Device>,
    lib_input: Libinput,
    active_tab: AppTab,
    configured_dpi: f64,
    x_motion: f64,
    abs_motion: f64,

    //basic
    key_reset_x: KeyBindState,
    key_reset_abs: KeyBindState,
    x_motion_base: f64,
    abs_motion_base: f64,

    //record
    record_recorder: Recorder,
    revolutions: f64,
    current_sensitivity: f64,
    target_rpi: f64,
}

struct Recorder {
    recording: bool,
    value: f64,
    key_bind: KeyBindState,
}

impl Recorder {
    fn ui(&mut self, ui: &mut Ui, acc_motion: f64) -> f64 {
        if MyEguiApp::key_bind_button(
            ui,
            if self.recording { "start" } else { "stop" },
            &mut self.key_bind,
        ) {
            if self.recording {
                self.value = acc_motion - self.value
            } else {
                self.value = acc_motion;
            }
            self.recording = !self.recording;
        }
        if self.recording {
            acc_motion - self.value
        } else {
            self.value
        }
    }
}

impl MyEguiApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut input = Libinput::new_with_udev(Interface);
        input.udev_assign_seat("seat0").unwrap();
        MyEguiApp {
            mouse_states: HashSet::new(),
            lib_input: input,
            active_mouse: None,
            active_tab: AppTab::Record,
            configured_dpi: f64::NAN,
            abs_motion: 0.0,
            x_motion: 0.0,

            key_reset_x: KeyBindState::Unbound,
            key_reset_abs: KeyBindState::Unbound,
            x_motion_base: 0.0,
            abs_motion_base: 0.0,

            record_recorder: Recorder {
                value: 0.0,
                key_bind: KeyBindState::Unbound,
                recording: false,
            },
            target_rpi: f64::NAN,
            current_sensitivity: f64::NAN,
            revolutions: f64::NAN,
        }
    }

    fn mouse_combo_box_string(dev: Option<&Device>) -> String {
        if let Some(x) = dev {
            format!("{}, {}", x.name(), x.sysname())
        } else {
            "Select Mouse".into()
        }
    }

    fn input(ui: &mut Ui, label: &str, v: &mut f64, f: impl FnOnce(DragValue) -> DragValue) {
        ui.horizontal(|ui| {
            ui.label(label);
            f(DragValue::new(v)).ui(ui);
        });
    }

    fn key_bind_button(ui: &mut Ui, label: &str, key_bind_state: &mut KeyBindState) -> bool {
        ui.horizontal(|ui| {
            let button_triggered = ui.button(label).clicked();
            match *key_bind_state {
                KeyBindState::Binding => {
                    if ui.button("cancel").clicked() {
                        *key_bind_state = KeyBindState::Unbound
                    }
                }
                KeyBindState::Bound(k, _) => {
                    if ui.button(format!("key {k}")).clicked() {
                        *key_bind_state = KeyBindState::Binding
                    }
                }
                KeyBindState::Unbound => {
                    if ui.button("bind").clicked() {
                        *key_bind_state = KeyBindState::Binding
                    }
                }
            }
            button_triggered || key_bind_state.poll_triggered()
        })
        .inner
    }

    fn tab_basic(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("x motion");
            ui.label(format!("{}", self.x_motion - self.x_motion_base));
            if Self::key_bind_button(ui, "reset", &mut self.key_reset_x) {
                self.x_motion_base = self.x_motion;
            }
        });

        ui.horizontal(|ui| {
            ui.label("abs motion");
            ui.label(format!("{}", self.abs_motion - self.abs_motion_base));
            if Self::key_bind_button(ui, "reset", &mut self.key_reset_abs) {
                self.abs_motion_base = self.abs_motion_base;
            }
        });
    }

    fn show_value(ui: &mut Ui, name: &str, value: f64) {
        ui.horizontal(|ui| {
            ui.label(name);
            if value.is_nan() {
                ui.label("missing input");
            } else {
                ui.label(format!("{:.3?}", value))
                    .on_hover_text(format!("{:?}", value));
            }
            if ui
                .add_enabled(!value.is_nan(), Button::new("copy"))
                .clicked()
            {
                ui.ctx().copy_text(format!("{:?}", value));
            }
        });
    }

    fn tab_record(&mut self, ui: &mut Ui) {
        let dots = self.record_recorder.ui(ui, self.x_motion);
        let inch = dots / self.configured_dpi;
        let current_rpi = (self.revolutions as f64 / inch).abs();
        let rpd = (self.revolutions as f64 / dots).abs();
        let rdp1 = rpd / self.current_sensitivity;
        let adjusted_sensitivity = self.current_sensitivity * (self.target_rpi / current_rpi);
        ui.group(|ui| {
            ui.label("inputs");
            Self::input(ui, "mouse dpi", &mut self.configured_dpi, |d| d.speed(10.0));
            Self::input(
                ui,
                "current sensitivity",
                &mut self.current_sensitivity,
                |d| d.speed(0.02),
            );
            Self::input(ui, "number of revolutions", &mut self.revolutions, |d| {
                d.speed(0.05).max_decimals(0)
            });
            Self::input(
                ui,
                "target revolutions per inch",
                &mut self.target_rpi,
                |d| d.speed(0.02),
            );
        });
        ui.group(|ui| {
            ui.label("outputs");
            Self::show_value(ui, "total motion", dots);
            Self::show_value(ui, "revolutions per inch", current_rpi);
            Self::show_value(ui, "revolutions per dot", rpd);
            Self::show_value(ui, "revolutions per dot at sensitivity=1", rdp1);
            Self::show_value(ui, "adjusted sensitivity", adjusted_sensitivity);
            Self::show_value(ui, "sensitivity adjustment", self.target_rpi / current_rpi);
        });
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        self.lib_input.dispatch().unwrap();
        'next_event: for event in &mut self.lib_input {
            match &event {
                Event::Pointer(PointerEvent::Motion(e)) => {
                    let device = e.device();
                    if self.active_mouse.as_ref() == Some(&device) {
                        self.x_motion += e.dx_unaccelerated();
                        self.abs_motion += f64::hypot(e.dx_unaccelerated(), e.dy_unaccelerated());
                    }
                    self.mouse_states.insert(device);
                }
                Event::Keyboard(e) => {
                    if e.key_state() == KeyState::Pressed {
                        for k in [
                            &mut self.key_reset_x,
                            &mut self.key_reset_abs,
                            &mut self.record_recorder.key_bind,
                        ] {
                            if let KeyBindState::Binding = *k {
                                *k = KeyBindState::Bound(e.key(), false);
                                continue 'next_event;
                            }
                            if let KeyBindState::Bound(b, p) = k {
                                if *b == e.key() {
                                    *p = true;
                                }
                            }
                        }
                    }
                }
                _ => {}
            };
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.active_mouse.is_none() {
                self.active_mouse = self.mouse_states.iter().next().cloned();
            }
            ui.horizontal(|ui| {
                egui::ComboBox::from_label("mouse")
                    .selected_text(Self::mouse_combo_box_string(self.active_mouse.as_ref()))
                    .show_ui(ui, |ui| {
                        for dev in self.mouse_states.iter() {
                            ui.selectable_value(
                                &mut self.active_mouse,
                                Some(dev.clone()),
                                Self::mouse_combo_box_string(Some(dev)),
                            );
                        }
                    });
                ui.selectable_value(&mut self.active_tab, AppTab::Basic, "basic");
                ui.selectable_value(&mut self.active_tab, AppTab::Record, "record");
            });

            match self.active_tab {
                AppTab::Basic => self.tab_basic(ui),
                AppTab::Record => self.tab_record(ui),
            }
        });
    }
}
