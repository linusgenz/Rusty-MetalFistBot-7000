use serenity::all::{CommandInteraction, Context};
use serenity::builder::CreateCommand;
use serenity::model::application::ResolvedOption;
use std::error::Error;

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
    _options: &[ResolvedOption<'_>],
) -> String {
    
    "Left the voice channel".to_string()
}

pub fn register() -> CreateCommand {
    CreateCommand::new("leave").description("Leave the voice channel")
}
