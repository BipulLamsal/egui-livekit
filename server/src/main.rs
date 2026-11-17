use livekit_api::services::LiveKitApi;

const APP_KEY: &str = "devkey";
const APP_SECRET: &str = "secret";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = LiveKitApi::with_api_key("http://127.0.0.1:7880", APP_KEY, APP_SECRET);
    // there is no event listener to listen in livekit???
    //  :(

    Ok(())
}
