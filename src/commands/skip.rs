use serenity::all::{CommandInteraction, Context};
use serenity::builder::CreateCommand;
use serenity::model::application::ResolvedOption;
use std::error::Error;

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
    _options: &[ResolvedOption<'_>],
) -> String {
    
    "currently not supported".to_string()
}

pub fn register() -> CreateCommand {
    CreateCommand::new("skip").description("Skip the currently playing song")
}
