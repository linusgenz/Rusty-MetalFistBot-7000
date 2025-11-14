use crate::BotData;
use crate::discord_voice_api::voice::audio_commands::AudioCommand;
use serenity::all::{CommandInteraction, Context, CreateEmbed};
use serenity::builder::CreateCommand;
use std::error::Error;

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
        .filter_cmd_tx
        .send(AudioCommand::ToggleBassBoost(new_state))
        .await
        .expect("Filter channel invalid");

    let status_text = if new_state { "enabled" } else { "disabled" };
    CreateEmbed::new().title(format!("Bass-boost **{}**", status_text))
}

pub fn register() -> CreateCommand {
    CreateCommand::new("bass-boost").description("Apply a bass-boost effect on the playing music")
}
