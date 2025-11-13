use crate::{BotData, QuoteData};
use rand::prelude::IndexedRandom;
use rand::seq::IteratorRandom;
use serenity::all::{
    CommandInteraction, Context, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter,
};
use serenity::builder::CreateCommand;
use std::error::Error;

pub async fn run(ctx: &Context, command: &CommandInteraction) -> CreateEmbed {
    let data = ctx.data.read().await;
    let quotes = data.get::<QuoteData>().unwrap();

    let quote = quotes.choose(&mut rand::rng()).unwrap();

    let data = ctx.data.read().await;
    let bot_user = data.get::<BotData>();

    let bot_avatar = bot_user.map(|b| b.bot_pfp_url.clone()).unwrap_or_default();

    CreateEmbed::new()
        .color(0xFFD700)
        .author(CreateEmbedAuthor::new(
            "Rusty MetalFistBot 7000 | Random Quote",
        ))
        .field("🗣️ Quote", &quote.text, false)
        .field("✍️ Author", format!("||{}||", &quote.author), true)
        .field("📅 Date", &quote.date, true)
        .footer(
            CreateEmbedFooter::new(format!("Requested by {}", command.user.name))
                .icon_url(bot_avatar),
        )
}
pub fn register() -> CreateCommand {
    CreateCommand::new("rand-quote").description("Random quote")
}
