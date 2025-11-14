use serenity::all::{CommandInteraction, Context, CreateEmbed};
use serenity::builder::CreateCommand;
use serenity::model::application::ResolvedOption;
use std::error::Error;
use crate::BotData;
use crate::discord_voice_api::voice::audio_commands::AudioCommand;

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
) -> CreateEmbed {
    let guild_id = match command.guild_id {
        Some(g) => g.to_string(),
        None => return CreateEmbed::new().title("❌ Not in a guild"),
    };

    let data_read = ctx.data.read().await;
    let voice_api = data_read
        .get::<BotData>()
        .expect("BotData missing")
        .voice_api
        .clone();

    let player = match voice_api.get_player(&guild_id).await {
        Some(p) => p,
        None => return CreateEmbed::new()
            .title("❌ Not connected to voice")
            .description("Use `/play` to connect the bot to a voice channel"),
    };

    let current_state = player.audio_filter_state.read().await;
    let new_state = !current_state.bass_boost;
    drop(current_state);

    player
        .playback_cmd_tx
        .send(AudioCommand::Skip)
        .await
        .expect("Playback channel invalid");

    CreateEmbed::new().title("Track skipped")
}

pub fn register() -> CreateCommand {
    CreateCommand::new("skip").description("Skip the currently playing song")
}
