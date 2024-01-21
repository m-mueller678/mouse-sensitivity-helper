use std::collections::HashMap;
use input::{Device, Event, Libinput, LibinputInterface};
use libc::{O_RDONLY, O_RDWR, O_WRONLY};
use std::fs::{File, OpenOptions};
use std::os::unix::{fs::OpenOptionsExt, io::OwnedFd};
use std::path::Path;
use std::sync::{Arc, Mutex};
use input::event::{EventTrait, PointerEvent};
use input::event::keyboard::{KeyboardEventTrait, KeyState};

struct Interface;

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

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native("My egui App", native_options, Box::new(|cc| Box::new(MyEguiApp::new(cc))));
}

#[derive(Default)]
struct MouseData {
    x_motion:f64,
    abs_motion:f64,
}

struct MyEguiApp {
    mouse_states:HashMap<Device, MouseData>,
    keys:Vec<u32>,
    active_mouse:Option<Device>,
    lib_input:Libinput,
}

impl MyEguiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut input = Libinput::new_with_udev(Interface);
        input.udev_assign_seat("seat0").unwrap();
        MyEguiApp{
            mouse_states:HashMap::new(),
            lib_input:input,
            keys:Vec::new(),
            active_mouse:None,
        }
    }

    fn mouse_combo_box_string(dev:Option<&Device>)->String{
        if let Some(x)=dev{
            format!("{}, {}",x.name(),x.sysname())
        }else{
            "Select Mouse".into()
        }
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame){
        ctx.request_repaint();
        self.lib_input.dispatch().unwrap();
        for event in &mut self.lib_input {
            match &event{
                Event::Pointer(PointerEvent::Motion(e))=>{
                    let s=self.mouse_states.entry(e.device()).or_default();
                    s.x_motion+=e.dx_unaccelerated();
                    s.abs_motion+=f64::hypot(e.dx_unaccelerated(),e.dy_unaccelerated());
                }
                Event::Keyboard(e)=>{
                    if e.key_state() == KeyState::Pressed{
                        dbg!(e.key());
                        self.keys.push(e.key());
                    }
                }
                _=>{},
            };
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.active_mouse.is_none(){
                self.active_mouse=self.mouse_states.keys().cloned().next();
            }
            egui::ComboBox::from_label("mouse")
                .selected_text(Self::mouse_combo_box_string(self.active_mouse.as_ref())).show_ui(ui,|ui|{
                for (dev,_) in &self.mouse_states{
                    ui.selectable_value(&mut self.active_mouse,Some(dev.clone()),Self::mouse_combo_box_string(Some(dev)));
                }
            });

            if let Some(selected)=&self.active_mouse{
                let state = self.mouse_states.get_mut(selected).unwrap();
                ui.label("x motion");
                ui.label(format!("{}",state.x_motion));
                ui.label("abs motion");
                ui.label(format!("{}",state.abs_motion));
            }
        });
    }
}