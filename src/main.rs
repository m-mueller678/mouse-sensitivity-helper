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
        OpenOptions::new()
            .custom_flags(flags)
            .read((flags & O_RDONLY != 0) | (flags & O_RDWR != 0))
            .write((flags & O_WRONLY != 0) | (flags & O_RDWR != 0))
            .open(path)
            .map(|file| file.into())
            .map_err(|err| err.raw_os_error().unwrap())
    }
    fn close_restricted(&mut self, fd: OwnedFd) {
        drop(File::from(fd));
    }
}

use eframe::egui;
use egui::{Button, ComboBox, DragValue, Ui, Widget};
use egui_extras::{Column, TableBody, TableBuilder};

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

struct MyEguiApp {
    mouse_states: HashSet<Device>,
    active_mouse: Option<Device>,
    lib_input: Libinput,
    configured_dpi: f64,
    x_motion: f64,
    abs_motion: f64,
    key_bind: KeyBindState,
    recording: bool,
    revolutions: f64,
    current_sensitivity: f64,
    target_rpi: f64,
    distance_moved: f64,
    distnance_moved_is_inch: bool,
}

impl MyEguiApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut input = Libinput::new_with_udev(Interface);
        input.udev_assign_seat("seat0").unwrap();
        MyEguiApp {
            mouse_states: HashSet::new(),
            lib_input: input,
            active_mouse: None,
            configured_dpi: f64::NAN,
            abs_motion: 0.0,
            x_motion: 0.0,
            target_rpi: f64::NAN,
            current_sensitivity: f64::NAN,
            key_bind: KeyBindState::Unbound,
            recording: false,
            revolutions: f64::NAN,
            distance_moved: f64::NAN,
            distnance_moved_is_inch: false,
        }
    }

    fn mouse_combo_box_string(dev: Option<&Device>) -> String {
        if let Some(x) = dev {
            format!("{}, {}", x.name(), x.sysname())
        } else {
            "Select Mouse".into()
        }
    }

    fn input(
        body: &mut TableBody,
        label: &str,
        v: &mut f64,
        f: impl FnOnce(DragValue) -> DragValue,
    ) {
        body.row(30.0, |mut row| {
            row.col(|ui| {
                ui.label(label);
            });
            row.col(|ui| {
                if f(Self::default_drag_value(v)).ui(ui).dragged() {
                    if !v.is_finite() {
                        *v = 1.0;
                    }
                }
            });
        });
    }

    fn default_drag_value(v: &mut f64) -> DragValue {
        DragValue::new(v)
            .clamp_range(0.0..=f64::INFINITY)
            .custom_formatter(|v, d| {
                if v.is_finite() {
                    format!("{v:.*}", *d.end())
                } else {
                    "---".into()
                }
            })
            .custom_parser(|s| match s.parse::<f64>() {
                Ok(x) => Some(x),
                Err(_) => Some(f64::NAN),
            })
            .update_while_editing(false)
    }

    fn show_value(body: &mut TableBody, name: &str, value: f64) {
        body.row(30.0, |mut row| {
            row.col(|ui| {
                ui.label(name);
            });
            row.col(|ui| {
                if value.is_nan() {
                    ui.label("missing input");
                } else {
                    ui.label(format!("{:.3?}", value))
                        .on_hover_text(format!("{:?}", value));
                }
            });
            row.col(|ui| {
                if ui
                    .add_enabled(!value.is_nan(), Button::new("copy"))
                    .clicked()
                {
                    ui.ctx().copy_text(format!("{:?}", value));
                }
            });
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

    fn inputs_outputs(&mut self, ui: &mut Ui) {
        let dots = self.x_motion;
        let physical_distance = self.distance_moved
            * if self.distnance_moved_is_inch {
                1.0
            } else {
                1.0 / 2.54
            };
        let inch = dots / self.configured_dpi;
        let current_rpi = (self.revolutions / inch).abs();
        let rpd = (self.revolutions as f64 / dots).abs();
        let rdp1 = rpd / self.current_sensitivity;
        let adjusted_sensitivity = self.current_sensitivity * (self.target_rpi / current_rpi);

        ui.group(|ui| {
            ui.heading("inputs");
            TableBuilder::new(ui)
                .column(Column::exact(200.0))
                .column(Column::remainder())
                .body(|mut body| {
                    Self::input(&mut body, "mouse dpi", &mut self.configured_dpi, |d| {
                        d.speed(10.0)
                    });
                    Self::input(
                        &mut body,
                        "current sensitivity",
                        &mut self.current_sensitivity,
                        |d| d.speed(0.02),
                    );
                    Self::input(
                        &mut body,
                        "number of revolutions",
                        &mut self.revolutions,
                        |d| d.speed(0.05).max_decimals(0),
                    );
                    Self::input(
                        &mut body,
                        "target revolutions per inch",
                        &mut self.target_rpi,
                        |d| d.speed(0.02),
                    );
                    body.row(30.0, |mut row| {
                        row.col(|ui| {
                            ui.label("physical distance");
                        });
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                if Self::default_drag_value(&mut self.distance_moved)
                                    .speed(0.02)
                                    .ui(ui)
                                    .dragged()
                                    && !self.distance_moved.is_finite()
                                {
                                    self.distance_moved = 1.0;
                                }
                                ComboBox::new("physical_distance_unit", "")
                                    .selected_text(if self.distnance_moved_is_inch {
                                        "inch"
                                    } else {
                                        "cm"
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.distnance_moved_is_inch,
                                            false,
                                            "cm",
                                        );
                                        ui.selectable_value(
                                            &mut self.distnance_moved_is_inch,
                                            true,
                                            "inch",
                                        );
                                    });
                            });
                        });
                    });
                });
        });

        for x in [
            &mut self.configured_dpi,
            &mut self.current_sensitivity,
            &mut self.distance_moved,
            &mut self.revolutions,
            &mut self.target_rpi,
        ] {
            if !x.is_finite() {
                *x = f64::NAN;
            }
        }

        ui.group(|ui| {
            ui.push_id("outputs", |ui| {
                ui.heading("outputs");
                TableBuilder::new(ui)
                    .column(Column::exact(200.0))
                    .column(Column::exact(200.0))
                    .column(Column::remainder())
                    .body(|mut body| {
                        Self::show_value(&mut body, "horizontal motion", dots);
                        Self::show_value(&mut body, "absolute motion", self.abs_motion);
                        Self::show_value(&mut body, "revolutions per inch", current_rpi);
                        Self::show_value(&mut body, "revolutions per dot", rpd);
                        Self::show_value(&mut body, "revolutions per dot at sensitivity=1", rdp1);
                        Self::show_value(&mut body, "adjusted sensitivity", adjusted_sensitivity);
                        Self::show_value(
                            &mut body,
                            "sensitivity adjustment",
                            self.target_rpi / current_rpi,
                        );
                        Self::show_value(
                            &mut body,
                            "computed dpi",
                            self.abs_motion / physical_distance,
                        );
                    });
            })
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
                    if self.active_mouse.as_ref() == Some(&device) && self.recording {
                        self.x_motion += e.dx_unaccelerated();
                        self.abs_motion += f64::hypot(e.dx_unaccelerated(), e.dy_unaccelerated());
                    }
                    self.mouse_states.insert(device);
                }
                Event::Keyboard(e) => {
                    if e.key_state() == KeyState::Pressed {
                        for k in [&mut self.key_bind] {
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
                ComboBox::from_label("mouse")
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
                ui.label("recording:");
                if MyEguiApp::key_bind_button(
                    ui,
                    if self.recording { "stop" } else { "start" },
                    &mut self.key_bind,
                ) {
                    if !self.recording {
                        self.x_motion = 0.0;
                        self.abs_motion = 0.0;
                    }
                    self.recording = !self.recording;
                }
            });
            self.inputs_outputs(ui);
        });
    }
}
