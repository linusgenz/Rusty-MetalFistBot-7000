use crate::BotData;
use serenity::all::{
    CommandInteraction, Context, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter,
};
use serenity::builder::CreateCommand;
use serenity::model::application::ResolvedOption;
use std::error::Error;

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
    _options: &[ResolvedOption<'_>],
) -> CreateEmbed {
    let guild_id = match command.guild_id {
        Some(g) => g.to_string(),
        None => return CreateEmbed::new().title("❌ Not in a guild"),
    };

    let data_read = ctx.data.read().await;
    let bot_user = match data_read.get::<BotData>() {
        Some(b) => b,
        None => return CreateEmbed::new().title("❌ Bot data not found"),
    };

    let player = match bot_user.voice_api.get_player(&guild_id).await {
        Some(p) => p,
        None => {
            return CreateEmbed::new()
                .title("❌ Not connected to voice")
                .description("Use `/play` to connect the bot to a voice channel");
        }
    };

    let queue = player.get_queue();

    let current_track = match queue.get_current_track().await {
        Some(track) => track.clone(),
        None => {
            return CreateEmbed::new()
                .title("🎵 No track currently playing")
                .description("Add songs with `/play`!");
        }
    };

    let title = &current_track.title;
    let url = current_track.url.as_deref().unwrap_or("unknown");
    let desc = format!("[{}]({})\n", title, url);

    CreateEmbed::new()
        .title("🎶 Current track")
        .author(CreateEmbedAuthor::new("Rusty MetalFistBot 7000"))
        .footer(
            CreateEmbedFooter::new(format!("Requested by {}", command.user.name))
                .icon_url(bot_user.bot_pfp_url.clone()),
        )
        .thumbnail(current_track.thumbnail.clone().unwrap_or_default())
        .description(desc)
        .color(0xFF972C)
}

pub fn register() -> CreateCommand {
    CreateCommand::new("nowplaying").description("Show information, about the current track")
}
