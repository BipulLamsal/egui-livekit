#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![expect(rustdoc::missing_crate_level_docs)] // it's an example

use eframe::egui;

use client::{APP_KEY, APP_SECRET, APP_URL};
use egui::RichText;
use livekit::{Room, RoomEvent, RoomOptions, StreamReader, StreamTextOptions, StreamWriter};
use livekit_api::access_token;
use log::error;
use tokio::{
    runtime,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

#[derive(Default)]
enum Page {
    #[default]
    First,
    Error(String),
    Main,
}

enum Command {
    Join { username: String, room_name: String },
}

enum UiEvent {
    Joined,
    JoinFailed(String),
}

struct MyApp {
    /// runtime within the lifecycle of myapp. we can still spawn an async task inbetween
    _rt: runtime::Runtime,
    cmd_tx: UnboundedSender<Command>,
    event_rx: UnboundedReceiver<UiEvent>,
    page: Page,
    username: String,
    room_name: String,
}

struct LiveKitState {
    joined: bool,
    room: Option<Room>,
    room_events: Option<UnboundedReceiver<RoomEvent>>,
}

impl MyApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rt = runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        // commands to send the egui thread from livekit thread
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        // events going back from the livekit thread to the egui thread
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();

        rt.spawn(livekit_worker(cmd_rx, event_tx, cc.egui_ctx.clone()));

        Self {
            _rt: rt,
            cmd_tx,
            event_rx,
            page: Page::First,
            username: String::new(),
            room_name: String::new(),
        }
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                UiEvent::Joined => self.page = Page::Main,
                UiEvent::JoinFailed(msg) => self.page = Page::Error(msg),
            }
        }

        egui::CentralPanel::default().show(ui, |ui| match &self.page {
            Page::Main => {
                egui::Panel::left("participants")
                    .min_size(200.0)
                    .max_size(300.0)
                    .show_separator_line(true)
                    .show(ui, |ui| {});
            }
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
                        let _ = self.cmd_tx.send(Command::Join {
                            username: self.username.clone(),
                            room_name: self.room_name.clone(),
                        });
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

enum WorkerState {
    Idle,
    Joined {
        room: Room,
        room_events: UnboundedReceiver<RoomEvent>,
    },
}

fn notify(event_tx: &UnboundedSender<UiEvent>, egui_ctx: &egui::Context, event: UiEvent) {
    if event_tx.send(event).is_ok() {
        // redraws the after the success
        egui_ctx.request_repaint();
    }
}

async fn livekit_worker(
    mut cmd_rx: UnboundedReceiver<Command>,
    event_tx: UnboundedSender<UiEvent>,
    egui_ctx: egui::Context,
) {
    let mut livekit_state = LiveKitState {
        room_events: None,
        room: None,
        joined: false,
    };

    while let Some(command) = cmd_rx.recv().await {
        match command {
            Command::Join {
                username,
                room_name,
            } => {
                if livekit_state.joined {
                    notify(&event_tx, &egui_ctx, UiEvent::Joined);
                }
                let token = match create_token(&username, &room_name) {
                    Ok(token) => token,
                    Err(e) => {
                        error!("token failed: {e}");
                        notify(&event_tx, &egui_ctx, UiEvent::JoinFailed(e.to_string()));
                        continue;
                    }
                };

                match Room::connect(APP_URL, &token, RoomOptions::default()).await {
                    Ok((room, room_events)) => {
                        livekit_state.room_events = Some(room_events);
                        livekit_state.room = Some(room);
                        notify(&event_tx, &egui_ctx, UiEvent::Joined);
                    }
                    Err(e) => {
                        error!("connect failed: {e}");
                        notify(&event_tx, &egui_ctx, UiEvent::JoinFailed(e.to_string()));
                    }
                }
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };

    eframe::run_native(
        "voicecall",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)))),
    )?;

    Ok(())
}
