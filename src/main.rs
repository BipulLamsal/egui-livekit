#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![expect(rustdoc::missing_crate_level_docs)] // it's an example

use eframe::egui;

use client::{APP_KEY, APP_SECRET};
use egui::RichText;
use livekit::{Room, RoomEvent, StreamReader, StreamTextOptions, StreamWriter};
use livekit_api::access_token;
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Default)]
enum Page {
    #[default]
    First,
    Error(String),
    Main,
}

#[derive(Default)]
struct MyApp {
    room_joined: bool,
    page: Page,
    username: String,
    room_name: String,
}

impl MyApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_global_style.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self::default()
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| match &self.page {
            Page::Main => {}
            Page::First => {
                ui.heading("Voice App");
                ui.horizontal(|ui| {
                    let name_label = ui.label("username: ");
                    ui.text_edit_singleline(&mut self.username)
                        .labelled_by(name_label.id);
                });

                ui.horizontal(|ui| {
                    let room_name_label = ui.label("room name: ");
                    ui.text_edit_singleline(&mut self.room_name)
                        .labelled_by(room_name_label.id);
                });

                ui.horizontal(|ui| {
                    if ui.button("Join!").clicked() {
                        self.page = Page::Error("Join not implemented!".to_string());
                    }
                    if ui.button("Create!").clicked() {
                        self.page = Page::Error("Create not implemented!".to_string());
                    }
                });
            }
            Page::Error(msg) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(msg));
                });

                ui.horizontal(|ui| {
                    if ui.button("close!").clicked() {
                        ui.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            }
        });
    }
}
fn create_token(username: &str, room_name: &str) -> Result<String, access_token::AccessTokenError> {
    let token = access_token::AccessToken::with_api_key(APP_KEY, APP_SECRET)
        .with_identity(username)
        .with_name(username)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: room_name.to_string(),
            ..Default::default()
        })
        .to_jwt();
    return token;
}

async fn run_sender(room: Room) -> Result<(), Box<dyn std::error::Error>> {
    println!("Running as sender");
    let options = StreamTextOptions::new_with_topic("test-topic");
    let writer = room.local_participant().stream_text(options).await?;
    println!("Opened the stream!");

    writer
        .write("Hello world! This is a message!\n".into())
        .await?;

    writer.close().await?;
    println!("Stream completed!");
    Ok(())
}

async fn run_receiver(
    _room: Room,
    mut room_event: UnboundedReceiver<RoomEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Running as receiver");
    while let Some(msg) = room_event.recv().await {
        match msg {
            RoomEvent::TextStreamOpened {
                reader,
                topic,
                participant_identity,
            } => {
                if topic == "test-topic" {
                    if let Some(reader) = reader.take() {
                        let result = reader.read_all().await?;
                        println!("msg received: {:?}", result);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "voicecall",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)))),
    )?;

    Ok(())
}
