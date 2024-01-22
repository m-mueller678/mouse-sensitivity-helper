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
use egui::Ui;

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
}

struct MyEguiApp {
    mouse_states: HashSet<Device>,
    active_mouse: Option<Device>,
    lib_input: Libinput,
    active_tab: AppTab,

    key_reset_x: KeyBindState,
    key_reset_abs: KeyBindState,
    x_motion: f64,
    abs_motion: f64,
}

impl MyEguiApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut input = Libinput::new_with_udev(Interface);
        input.udev_assign_seat("seat0").unwrap();
        MyEguiApp {
            mouse_states: HashSet::new(),
            lib_input: input,
            active_mouse: None,
            active_tab: AppTab::Basic,

            key_reset_x: KeyBindState::Unbound,
            key_reset_abs: KeyBindState::Unbound,
            x_motion: 0.0,
            abs_motion: 0.0,
        }
    }

    fn mouse_combo_box_string(dev: Option<&Device>) -> String {
        if let Some(x) = dev {
            format!("{}, {}", x.name(), x.sysname())
        } else {
            "Select Mouse".into()
        }
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

    fn basic_tab(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("x motion");
            ui.label(format!("{}", self.x_motion));
            if Self::key_bind_button(ui, "reset", &mut self.key_reset_x) {
                self.x_motion = 0.0;
            }
        });

        ui.horizontal(|ui| {
            ui.label("abs motion");
            ui.label(format!("{}", self.abs_motion));
            if Self::key_bind_button(ui, "reset", &mut self.key_reset_abs) {
                self.abs_motion = 0.0;
            }
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
                        for k in [&mut self.key_reset_x, &mut self.key_reset_abs] {
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
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, AppTab::Basic, "basic");
            });

            match self.active_tab {
                AppTab::Basic => self.basic_tab(ui),
            }
        });
    }
}
