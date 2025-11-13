use crate::BotData;
use serenity::all::{
    CommandInteraction, Context, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter,
};
use serenity::builder::CreateCommand;
use std::error::Error;

pub async fn run(ctx: &Context, command: &CommandInteraction) -> CreateEmbed {
    let guild_id = match command.guild_id {
        Some(g) => g.to_string(),
        None => return CreateEmbed::new().title("❌ Not in a guild"),
    };

    let data_read = ctx.data.read().await;
    let bot_user = data_read.get::<BotData>();

    let player = match bot_user.unwrap().voice_api.get_player(&guild_id).await {
        Some(p) => p,
        None => {
            return CreateEmbed::new()
                .title("❌ Not connected to voice")
                .description("Use `/play` to connect the bot to a voice channel");
        }
    };

    let queue = player.get_queue();

    if player.get_queue().is_empty().await {
        return CreateEmbed::new()
            .title("🎵 Queue is empty")
            .description("Add songs with `/play`!");
    }

    let mut desc = String::new();
    for (i, track) in queue.iter().await.into_iter().take(20).enumerate() {
        let title = &track.title;
        let url = track.url.as_deref().unwrap_or("unknown");
        desc.push_str(&format!("**{}.** [{}]({})\n", i + 1, title, url));
    }

    let bot_avatar = bot_user.map(|b| b.bot_pfp_url.clone()).unwrap_or_default();

    CreateEmbed::new()
        .title("🎶 Current queue")
        .description(desc)
        .color(0xFF972C)
        .footer(
            CreateEmbedFooter::new(format!("Requested by {}", command.user.name))
                .icon_url(bot_avatar),
        )
        .author(CreateEmbedAuthor::new("Rusty MetalFistBot 7000"))
}

pub fn register() -> CreateCommand {
    CreateCommand::new("queue").description("Show the current queue")
}
