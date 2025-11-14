use crate::BotData;
use crate::discord_voice_api::voice::player::Track;
use anyhow::Result;
use serde_json::Value;
use serenity::all::{
    ChannelId, CommandInteraction, CommandOptionType, Context, CreateCommandOption, GuildId,
    ResolvedValue,
};
use serenity::builder::CreateCommand;
use serenity::futures::StreamExt;
use serenity::model::application::ResolvedOption;
use tokio::process::Command;

fn is_playlist_url(url: &str) -> bool {
    url.contains("list=")
}

pub async fn fetch_youtube_metadata(video_url: &str) -> Result<Track> {
    let output = Command::new("yt-dlp")
        .arg("-j") // JSON output
        .arg("-f")
        .arg("bestaudio[ext=m4a]/bestaudio/best")
        .arg(video_url)
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "yt-dlp failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let mut data: Track = serde_json::from_slice(&output.stdout)?;

    if data.url.is_none() {
        let url_out = Command::new("yt-dlp")
            .arg("-f")
            .arg("bestaudio[ext=m4a]/bestaudio/best")
            .arg("-g") // get direct media URL
            .arg(video_url)
            .output()
            .await?;

        if url_out.status.success() {
            let stream_url = String::from_utf8_lossy(&url_out.stdout).trim().to_string();
            data.url = Some(stream_url);
        }
    }

    Ok(data)
}

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
    _options: &[ResolvedOption<'_>],
) -> String {
    let mut guild_id: GuildId = Default::default();
    let mut channel_id: ChannelId = Default::default();

    if let Some(g_id) = command.guild_id {
        guild_id = g_id;
        let user_id = command.user.id;

        let guild = match guild_id.to_guild_cached(&ctx.cache) {
            Some(g) => g,
            None => {
                return "internal error 501".to_string();
            }
        };

        if let Some(voice_state) = guild.voice_states.get(&user_id) {
            if let Some(ch_id) = voice_state.channel_id {
                channel_id = ch_id;
            } else {
                return "You have to be in a voice channel to use this command".to_string();
            }
        } else {
            return "You have to be in a voice channel to use this command".to_string();
        }
    }

    let token = std::env::var("DISCORD_TOKEN").expect("Error finding discord token");

    let url_option = _options.first();
    let url = match url_option {
        Some(option) => match &option.value {
            ResolvedValue::String(s) => s,
            _ => "Failed to parse url",
        },
        None => "Failed to parse url",
    };

    let data_read = ctx.data.read().await;
    let voice_api = data_read
        .get::<BotData>()
        .expect("BotData missing")
        .voice_api
        .clone();

    let player = voice_api
        .join(
            &token,
            guild_id.to_string().as_str(),
            channel_id.to_string().as_str(),
        )
        .await
        .expect("Could not connect to voice");

    let result_msg: String;

    if is_playlist_url(url) {
        match get_playlist_entries(url).await {
            Ok(entries) => {
                use futures::stream::{FuturesUnordered, StreamExt};

                let mut futures = FuturesUnordered::new();

                for (index, url) in entries.iter().enumerate() {
                    futures.push(async move {
                        let meta = fetch_youtube_metadata(&url).await;
                        (index, meta)
                    });
                }

                let mut results: Vec<(usize, Result<Track, _>)> = Vec::new();

                while let Some(r) = futures.next().await {
                    results.push(r);
                }

                results.sort_by_key(|(index, _)| *index);

                let mut count = 0;

                for (_, res) in results {
                    if let Ok(meta) = res {
                        let player_clone = player.clone();
                        player_clone.enqueue(meta).await;
                        count += 1;
                    }
                }

                result_msg = format!("Added {} tracks to queue", count);
            }
            Err(e) => {
                return format!("Konnte Playlist nicht laden: {}", e);
            }
        }
    } else {
        let meta = fetch_youtube_metadata(url).await.unwrap();
        let title = meta.title.clone();

        player.enqueue(meta).await;

        result_msg = format!("Added **{}** to queue", title);
    }

    result_msg
}

async fn get_playlist_entries(url: &str) -> Result<Vec<String>, String> {
    let output = Command::new("yt-dlp")
        .arg("--flat-playlist")
        .arg("-J")
        .arg(url)
        .output()
        .await
        .map_err(|e| format!("yt-dlp call failed: {e}"))?;

    if !output.status.success() {
        return Err("yt-dlp returned non-zero exit code".to_string());
    }

    let json: Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("JSON parse error: {e}"))?;

    let urls = json["entries"]
        .as_array()
        .ok_or("No playlist entries found")?
        .iter()
        .filter_map(|e| e["url"].as_str())
        .map(|id| id.to_string())
        .collect();

    Ok(urls)
}

pub fn register() -> CreateCommand {
    CreateCommand::new("play")
        .description("Play a song from youtube")
        .add_option(
            CreateCommandOption::new(CommandOptionType::String, "url", "Link to a youtube video")
                .required(true),
        )
}
