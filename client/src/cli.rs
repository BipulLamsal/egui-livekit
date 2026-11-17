use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct AppArgs {
    /// default is create room
    #[arg(short, long, default_value_t = false)]
    pub join: bool,

    /// your display username    
    #[arg(short, long)]
    pub username: String,

    /// room name    
    #[arg(short, long)]
    pub room: String,
}
