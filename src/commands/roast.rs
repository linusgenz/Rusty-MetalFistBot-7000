use crate::BotData;
use serenity::all::{
    CommandInteraction, Context, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter,
};
use serenity::builder::CreateCommand;
use std::error::Error;
use std::process::{Command, Stdio};

async fn generate_roast(user_name: String) -> Result<String, String> {
    // Prompt mit Usernamen
    /* let prompt = format!(
        "Schreibe einen lustigen, kreativen Roast für den User {}, er soll humorvoll sein und stark sowie provokant sein. Keine zusätzlichen Informationen sondern nur der Roast. Es handelt sich um einen Kommand eines Discord bots welche der User ({}) selbst Verwendet und geroasted werden möchte. Bitte maximal 3 Sätze. Kurz und prägnant. Keine zusätzlichen Sätze wie \"hier mein Versuch\" sondern direkt der Roast.",
        user_name, user_name
    );*/

    let prompt = format!(
            "Du bist ein sarkastischer Gamer, der spielerische, kreative Roasts über andere Spieler schreibt.
Erstelle einen lustigen, kurzen Roast über den User {}.
Benutze Gaming-Humor (z. B. „AFK“, „noob“, „laggt“), aber bleib freundlich und respektvoll.
2–3 Sätze, deutsch, humorvoll, nicht beleidigend. der Output kommt an einen Discord-Bot und wird nur Beleidigt wenn er den Kommand selbst nutzt, der User will also in dem Moment beleidigt/roasted werden. Baue mir einen Roast direkt ohne extra, nur der Roast fertig
",
        user_name
    );

    // Tokio Command starten (async)
    let output = Command::new("ollama")
        .arg("run")
        .arg("llama3.1:8b")
        .arg(&prompt)
        .stdout(Stdio::piped())
        .output()
        .map_err(|e| format!("Fehler beim Ausführen von Ollama: {e}"))?;

    if !output.status.success() {
        return Err("Ollama CLI hat einen Fehler zurückgegeben.".to_string());
    }

    let roast = String::from_utf8(output.stdout).map_err(|e| format!("UTF-8 Fehler: {e}"))?;

    Ok(roast.trim().to_string())
}

pub async fn run(ctx: &Context, command: &CommandInteraction) -> CreateEmbed {
    let result = generate_roast(command.user.name.clone())
        .await
        .unwrap_or_else(|err| format!("Fehler beim Generieren des Roasts: {}", err));

    let data = ctx.data.read().await;
    let bot_user = data.get::<BotData>();

    let bot_avatar = bot_user.map(|b| b.bot_pfp_url.clone()).unwrap_or_default();

    CreateEmbed::new()
        .color(0xFFD700)
        .author(CreateEmbedAuthor::new(
            "Rusty MetalFistBot 7000 | Random Quote",
        ))
        .description(result)
        .footer(
            CreateEmbedFooter::new(format!("Requested by {}", command.user.name))
                .icon_url(bot_avatar),
        )
}

pub fn register() -> CreateCommand {
    CreateCommand::new("roast").description("Get roasted")
}
