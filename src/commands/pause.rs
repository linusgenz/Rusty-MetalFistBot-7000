use serenity::all::{CommandInteraction, Context, CreateCommand, ResolvedOption};

pub async fn run(
    ctx: &Context,
    command: &CommandInteraction,
    _options: &[ResolvedOption<'_>],
) -> String {

    "Paused playback".to_string()
}

pub fn register() -> CreateCommand {
    CreateCommand::new("pause").description("pause the playback")
}
