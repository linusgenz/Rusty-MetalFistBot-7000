mod commands;
mod discord_voice_api;

use crate::discord_voice_api::DiscordVoiceApi;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serenity::all::{
    Command, CreateEmbed, Interaction,
};
use serenity::async_trait;
use serenity::builder::EditInteractionResponse;
use serenity::client::Context;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use std::fs;
use std::sync::Arc;
use tracing_subscriber::fmt::init;

struct Handler;
struct HttpKey;
impl TypeMapKey for HttpKey {
    type Value = HttpClient;
}

struct BotData {
    bot_pfp_url: String,
    voice_api: Arc<DiscordVoiceApi>,
}

impl TypeMapKey for BotData {
    type Value = BotData;
}

enum CommandResponse {
    Text(String),
    Embed(CreateEmbed),
}

pub struct QuoteData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub text: String,
    pub author: String,
    pub date: String,
}
impl TypeMapKey for QuoteData {
    type Value = Vec<Quote>;
}

const QUOTES_JSON: &str = include_str!("../data/quotes.json");
pub async fn load_quotes(ctx: &Context) -> Result<(), Box<dyn std::error::Error>> {
    let quotes: Vec<Quote> = serde_json::from_str(QUOTES_JSON)?;

    let mut data_write = ctx.data.write().await;
    data_write.insert::<QuoteData>(quotes);

    Ok(())
}



#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        load_quotes(&ctx).await.expect("Could not load quotes");

        {
            let mut data = ctx.data.write().await;
            data.insert::<BotData>(BotData {
                bot_pfp_url: ctx
                    .http
                    .get_current_user()
                    .await
                    .unwrap()
                    .avatar_url()
                    .unwrap(),
                voice_api: Arc::new(DiscordVoiceApi::new()),
            });
        }

         Command::set_global_commands(
            &ctx.http,
            vec![
                commands::ping::register(),
                commands::play::register(),
                commands::skip::register(),
                commands::pause::register(),
                commands::resume::register(),
                commands::leave::register(),
                commands::queue::register(),
                commands::nowplaying::register(),
                commands::neko::register(),
                commands::serverinfo::register(),
                commands::rand_quote::register(),
                commands::dick_size::register(),
                commands::roast::register(),
                commands::bass_boost::register()
            ],
        )
        .await
        .expect("Could not register commands");

        println!("✅ Logged in as {}", ready.user.name);
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            command.defer(&ctx.http).await.expect("Could not defer");

            let response = match command.data.name.as_str() {
                "ping" => Some(CommandResponse::Text(commands::ping::run(
                    &command.data.options(),
                ))),
                "play" => Some(CommandResponse::Text(
                    commands::play::run(&ctx, &command, &command.data.options()).await,
                )),
                "skip" => Some(CommandResponse::Embed(
                    commands::skip::run(&ctx, &command).await,
                )),
                "leave" => Some(CommandResponse::Text(
                    commands::leave::run(&ctx, &command, &command.data.options()).await,
                )),
                "resume" => Some(CommandResponse::Embed(
                    commands::resume::run(&ctx, &command).await,
                )),
                "pause" => Some(CommandResponse::Embed(
                    commands::pause::run(&ctx, &command).await,
                )),
                "queue" => Some(CommandResponse::Embed(
                    commands::queue::run(&ctx, &command).await,
                )),
                "neko" => Some(CommandResponse::Embed(
                    commands::neko::run(&ctx, &command).await,
                )),
                "nowplaying" => Some(CommandResponse::Embed(
                    commands::nowplaying::run(&ctx, &command, &command.data.options()).await,
                )),
                "serverinfo" => Some(CommandResponse::Embed(
                    commands::serverinfo::run(&ctx, &command).await,
                )),
                "rand-quote" => Some(CommandResponse::Embed(
                    commands::rand_quote::run(&ctx, &command).await,
                )),
                "dick-size" => Some(CommandResponse::Embed(
                    commands::dick_size::run(&ctx, &command).await,
                )),
                "roast" => Some(CommandResponse::Embed(
                    commands::roast::run(&ctx, &command).await,
                )),
                "bass-boost" => Some(CommandResponse::Embed(
                    commands::bass_boost::run(&ctx, &command).await,
                )),
                _ => Some(CommandResponse::Text("not implemented :(".to_string())),
            };

            if let Some(res) = response {
                let mut data = EditInteractionResponse::new();
                match res {
                    CommandResponse::Text(txt) => {
                        data = data.content(txt);
                    }
                    CommandResponse::Embed(embed) => {
                        data = data.add_embed(embed);
                    }
                }

                if let Err(why) = command.edit_response(&ctx.http, data).await {
                    println!("Cannot respond to slash command: {why}");
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    init();
    dotenvy::dotenv().ok();

    let token = std::env::var("DISCORD_TOKEN").expect("Error finding discord token");

    println!("token: {:?}", token);

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_PRESENCES
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_VOICE_STATES;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Error while creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}

/*
pub async fn fetch_youtube_audio() -> Result<String, Box<dyn Error>> {
    // Pfade zu Binaries (sollten ausführbar im Projektverzeichnis liegen)
    let libraries = Libraries::new(
        PathBuf::from("yt-dlp"),
        PathBuf::from("ffmpeg"),
    );

    let output_dir = PathBuf::from("output");
    let fetcher = Youtube::new(libraries, output_dir)?;

    // YouTube-Video-ID oder vollständige URL
    let url = "ufzSf1uMos8"; // oder "https://www.youtube.com/watch?v=ufzSf1uMos8"

    // Video abrufen
    match fetcher.get_video_by_id(url).await {
        Some(video) => {
            println!("🎬 Titel: {}", video.title);
            println!("📺 Kanal: {}", video.channel);

            // Beste Audio-URL extrahieren
            if let Some(best_audio) = video.best_audio_format().as_ref() {
                if let Some(url) = best_audio.download_info.url.as_ref() {
                    println!("🎵 Beste Audio-URL: {}", url);
                    return Ok(url.clone()); // <– URL zurückgeben
                }
            }

            Err("Keine gültige Audio-URL gefunden".into())
        }

        None => Err("Video konnte nicht gefunden werden".into()),
    }
}
*/
