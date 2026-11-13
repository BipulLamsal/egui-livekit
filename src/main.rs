use clap::Parser;
use client::cli::AppArgs;
use client::{APP_KEY, APP_SECRET};
use livekit::{Room, RoomEvent, RoomOptions};
use livekit_api::access_token;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let args = AppArgs::parse();
    let token = create_token(&args.username, &args.room)?;

    let (room, mut room_events) =
        Room::connect("http://127.0.0.1:7880", &token, RoomOptions::default()).await?;

    while let Some(event) = room_events.recv().await {
        match event {
            RoomEvent::ParticipantConnected(v) => {
                println!("hey {:?} just joined! say hello!", v.name());
            }
            RoomEvent::ParticipantDisconnected(v) => {
                println!("Bye bye! {:?}", v.name());
            }
            RoomEvent::ChatMessage {
                message,
                participant,
            } => {}

            _ => {}
        }
    }

    Ok(())
}
