#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![expect(rustdoc::missing_crate_level_docs)] // it's an example

use eframe::egui;

use client::{APP_KEY, APP_SECRET, APP_URL};
use egui::RichText;
use livekit::options::TrackPublishOptions;
use livekit::track::{LocalAudioTrack, LocalTrack, TrackSource};
use livekit::{
    PlatformAudio, Room, RoomEvent, RoomOptions, StreamReader, StreamTextOptions, StreamWriter,
};
use livekit_api::access_token;
use log::debug;
use log::error;
use log::info;
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
    Message(String),
}

enum UiEvent {
    Joined,
    JoinFailed(String),
    Participants(Vec<String>),
    Message((String, String)),
    Disconnected,
}

struct MyApp {
    /// runtime within the lifecycle of myapp. we can still spawn an async task inbetween
    _rt: runtime::Runtime,
    cmd_tx: UnboundedSender<Command>,
    event_rx: UnboundedReceiver<UiEvent>,
    page: Page,
    username: String,
    room_name: String,
    participants: Vec<String>,
    messages: Vec<(String, String)>,
    message_input_field: String,
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
            participants: Vec::new(),
            messages: Vec::new(),
            message_input_field: String::new(),
        }
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                UiEvent::Joined => {
                    self.page = Page::Main;
                }
                UiEvent::JoinFailed(msg) => self.page = Page::Error(msg),
                UiEvent::Participants(participants) => self.participants = participants,
                UiEvent::Message((user, msg)) => {
                    self.messages.push((user, msg));
                }

                UiEvent::Disconnected => {
                    self.participants.clear();
                    self.page = Page::Error("disconnected from room".to_string());
                }
            }
        }

        egui::CentralPanel::default().show(ui, |ui| match &self.page {
            Page::Main => {
                egui::Panel::left("participants")
                    .min_size(200.0)
                    .max_size(300.0)
                    .show_separator_line(true)
                    .show(ui, |ui| {
                        ui.heading("Participants");
                        ui.separator();

                        if self.participants.is_empty() {
                            ui.weak("no one yet");
                        }

                        for name in &self.participants {
                            ui.label(name);
                        }
                    });

                egui::Panel::bottom("message_input")
                    .show_separator_line(true)
                    .show(ui, |ui| {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.message_input_field)
                                .hint_text("type any message...")
                                .desired_width(f32::INFINITY),
                        );

                        let enter_pressed =
                            response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                        if enter_pressed {
                            let text = self.message_input_field.trim().to_owned();

                            if !text.is_empty() {
                                // streaming text is for other incoming receiver not to self
                                // so ui changes here is local and not based on event
                                self.messages.push(("you".to_string(), text.clone()));
                                let _ = self.cmd_tx.send(Command::Message(text));
                                self.message_input_field.clear();
                            }

                            response.request_focus();
                        }
                    });

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.messages.is_empty() {
                            ui.weak("no messages yet");
                        }
                        // every time rerenders
                        // bad by design
                        for (username, text) in &self.messages {
                            ui.label(format!("[{username}]: {text}"));
                        }
                    });
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

async fn send_message(room: &Room, messsage: String) -> Result<(), Box<dyn std::error::Error>> {
    let options = StreamTextOptions::new_with_topic("chat");
    let writer = room.local_participant().stream_text(options).await?;
    writer.write(&messsage).await?;
    writer.close().await?;

    debug!("message sent by: {}", room.local_participant().name());
    Ok(())
}

async fn publish_microphone(room: &Room, audio: &PlatformAudio) {
    if let Some(device) = audio.recording_devices().next() {
        if let Err(e) = audio.set_recording_device(&device.id) {
            error!("failed to select microphone: {e}");
        }
    } else {
        error!("no recording devices found");
    }

    if let Some(device) = audio.playout_devices().next() {
        if let Err(e) = audio.set_playout_device(&device.id) {
            error!("failed to select speaker: {e}");
        }
    } else {
        error!("no playout devices found");
    }

    let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
    let options = TrackPublishOptions {
        source: TrackSource::Microphone,
        ..Default::default()
    };

    if let Err(e) = room
        .local_participant()
        .publish_track(LocalTrack::Audio(track), options)
        .await
    {
        error!("failed to publish microphone: {e}");
    }
}

fn notify(event_tx: &UnboundedSender<UiEvent>, egui_ctx: &egui::Context, event: UiEvent) {
    if event_tx.send(event).is_ok() {
        // redraws the after the success
        egui_ctx.request_repaint();
    }
}

fn participant_names(room: &Room) -> Vec<String> {
    let local = room.local_participant();
    let mut names = vec![local.name()];

    let mut remotes: Vec<String> = room
        .remote_participants()
        .into_values()
        .map(|participant| participant.name())
        .collect();

    remotes.sort();
    names.extend(remotes);
    names
}

async fn livekit_worker(
    mut cmd_rx: UnboundedReceiver<Command>,
    event_tx: UnboundedSender<UiEvent>,
    egui_ctx: egui::Context,
) {
    let platform_audio = match PlatformAudio::new() {
        Ok(audio) => {
            info!("platform audio is ready!");
            Some(audio)
        }
        Err(e) => {
            error!("platform audio unavailable: {e}");
            None
        }
    };

    // first command should be join
    let (room, mut room_events) = loop {
        let Some(command) = cmd_rx.recv().await else {
            return;
        };

        match command {
            Command::Join {
                username,
                room_name,
            } => {
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
                        notify(&event_tx, &egui_ctx, UiEvent::Joined);
                        break (room, room_events);
                    }
                    Err(e) => {
                        error!("connect failed: {e}");
                        notify(&event_tx, &egui_ctx, UiEvent::JoinFailed(e.to_string()));
                    }
                }
            }
            _ => {}
        }
    };

    if let Some(audio) = &platform_audio {
        publish_microphone(&room, audio).await;
    }

    loop {
        tokio::select! {
            event = room_events.recv() => {
                let Some(event) = event else { break };
                match event {
                    RoomEvent::Connected { .. }
                    | RoomEvent::ParticipantConnected(_)
                    | RoomEvent::ParticipantDisconnected(_) => {
                        notify(
                            &event_tx,
                            &egui_ctx,
                            UiEvent::Participants(participant_names(&room)),
                        );
                    }

                    RoomEvent::TextStreamOpened {
                        reader,
                        topic,
                        participant_identity,
                    } => {
                        if topic == "chat" {
                            if let Some(reader) = reader.take() {
                                let username = room
                                    .remote_participants()
                                    .get(&participant_identity)
                                    .map(|participant| participant.name());

                                // only if username exists
                                if let Some(username) = username {
                                    if let Ok(data) = reader.read_all().await {
                                        notify(
                                            &event_tx,
                                            &egui_ctx,
                                            UiEvent::Message((username, data)),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                };

            }
            command = cmd_rx.recv() => {
                let Some(command) = command else { break };
                match command {
                    Command::Message(msg) => {
                        if send_message(&room, msg).await.is_err() {
                            error!("bruh! msg not sent!");
                        }
                    }
                    _ => {
                        // ignore ignore
                    }

                }

            }

        }
    }
    // disconnected if event is none
    notify(&event_tx, &egui_ctx, UiEvent::Disconnected);
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
