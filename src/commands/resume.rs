use serenity::all::{CommandInteraction, Context};
use serenity::builder::CreateCommand;
use serenity::model::application::ResolvedOption;
use std::error::Error;

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
    _options: &[ResolvedOption<'_>],
) -> String {


    "Resumed playback".to_string()
}

pub fn register() -> CreateCommand {
    CreateCommand::new("resume").description("resume the playback (when paused)")
}
