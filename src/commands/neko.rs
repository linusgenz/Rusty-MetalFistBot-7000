use crate::BotData;
use serde_json::Value;
use serenity::all::{
    CommandInteraction, Context, CreateCommand, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter,
};
const NEKO_URL: &str = "https://nekos.life/api/v2/img/neko";

pub async fn run(ctx: &Context, command: &CommandInteraction) -> CreateEmbed {
    let mut image_url =
        "https://www.gstatic.com/youtube/media/ytm/images/pbg/playlist-empty-state-@1200.png"
            .to_string();

    match reqwest::get(NEKO_URL).await {
        Ok(resp) => match resp.text().await {
            Ok(body) => match serde_json::from_str::<Value>(&body) {
                Ok(json) => {
                    if let Some(url) = json.get("url").and_then(|v| v.as_str()) {
                        image_url = url.to_string();
                    } else {
                        eprintln!("Key 'url' nicht gefunden im JSON: {}", body);
                    }
                }
                Err(err) => eprintln!("JSON Parsing Error: {}, STR: {}", err, body.as_str()),
            },
            Err(err) => eprintln!("Fehler beim Lesen des Response-Textes: {}", err),
        },
        Err(err) => eprintln!("HTTP Request Fehler: {}", err),
    }

    let data = ctx.data.read().await;
    let bot_user = data.get::<BotData>();

    let bot_avatar = bot_user.map(|b| b.bot_pfp_url.clone()).unwrap_or_default();

    

    CreateEmbed::new()
        .title("Neko")
        .author(CreateEmbedAuthor::new("Rusty MetalFistBot 7000"))
        .footer(
            CreateEmbedFooter::new(format!("Requested by {}", command.user.name))
                .icon_url(bot_avatar),
        )
        .image(&image_url)
        .color(0xFF972C)
}

pub fn register() -> CreateCommand {
    CreateCommand::new("neko").description("neko neko ne")
}
