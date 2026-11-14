use clap::Parser;
use client::cli::AppArgs;
use client::{APP_KEY, APP_SECRET};
use livekit::{Room, RoomEvent, RoomOptions, StreamReader, StreamTextOptions, StreamWriter};
use livekit_api::access_token;
use tokio::sync::mpsc::UnboundedReceiver;

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
    let args = AppArgs::parse();
    let token = create_token(&args.username, &args.room)?;

    let (room, room_events) =
        Room::connect("http://127.0.0.1:7880", &token, RoomOptions::default()).await?;

    if args.join {
        run_receiver(room, room_events).await?;
    } else {
        run_sender(room).await?;
    }

    Ok(())
}
