use crate::BotData;
use rand::Rng;
use rand::prelude::IndexedRandom;
use serenity::all::{
    CommandInteraction, Context, CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter,
};
use serenity::builder::CreateCommand;
use std::error::Error;

struct Unit<'a> {
    name: &'a str,
    factor: f64,
}

pub async fn run(ctx: &Context, command: &CommandInteraction) -> CreateEmbed {
    let data = ctx.data.read().await;
    let bot_user = data.get::<BotData>();

    let bot_avatar = bot_user.map(|b| b.bot_pfp_url.clone()).unwrap_or_default();

    let mut rng = rand::rng();
    let value: f64 = rng.random_range(1.0..=100.0);

    let units = [
        Unit {
            name: "Millimeter",
            factor: 0.001,
        },
        Unit {
            name: "Centimeter",
            factor: 0.01,
        },
        Unit {
            name: "Zoll",
            factor: 0.0254,
        },
        Unit {
            name: "Feet",
            factor: 0.3048,
        },
        Unit {
            name: "Yard",
            factor: 0.9144,
        },
        Unit {
            name: "Meter",
            factor: 1.0,
        },
        Unit {
            name: "Sandkörner",
            factor: 0.0005,
        },
        Unit {
            name: "Reiskörner",
            factor: 0.007,
        },
        Unit {
            name: "Ameisen",
            factor: 0.005,
        },
        Unit {
            name: "Kaffeebohnen",
            factor: 0.015,
        },
        Unit {
            name: "Gummibärchen",
            factor: 0.02,
        },
        Unit {
            name: "Smarties",
            factor: 0.018,
        },
        Unit {
            name: "Würfelzucker",
            factor: 0.016,
        },
        Unit {
            name: "Murmel",
            factor: 0.025,
        },
        Unit {
            name: "LEGO-Steine",
            factor: 0.032,
        },
        Unit {
            name: "Münzen",
            factor: 0.023,
        },
        Unit {
            name: "Lineale",
            factor: 0.3,
        },
        Unit {
            name: "Spaghetti",
            factor: 0.25,
        },
        Unit {
            name: "Bananen",
            factor: 0.15,
        },
        Unit {
            name: "USB-Sticks",
            factor: 0.07,
        },
        Unit {
            name: "Kabelbinder",
            factor: 0.4,
        },
        Unit {
            name: "Haferflockenpackungen",
            factor: 0.25,
        },
        Unit {
            name: "Tastaturen",
            factor: 0.45,
        },
        Unit {
            name: "Bildschirme",
            factor: 0.6,
        },
        Unit {
            name: "Gitarren",
            factor: 1.0,
        },
        Unit {
            name: "Lichtschwerter",
            factor: 1.0,
        },
        Unit {
            name: "Menschen",
            factor: 1.75,
        },
        Unit {
            name: "Kühlschränke",
            factor: 1.8,
        },
        Unit {
            name: "Autos",
            factor: 4.0,
        },
        Unit {
            name: "Sofas",
            factor: 2.0,
        },
        Unit {
            name: "Betten",
            factor: 2.1,
        },
        Unit {
            name: "Esstische",
            factor: 1.6,
        },
        Unit {
            name: "Türen",
            factor: 2.0,
        },
        Unit {
            name: "Busse",
            factor: 12.0,
        },
        Unit {
            name: "Container",
            factor: 6.0,
        },
        Unit {
            name: "Wale",
            factor: 25.0,
        },
        Unit {
            name: "Züge",
            factor: 40.0,
        },
        Unit {
            name: "Flugzeuge",
            factor: 70.0,
        },
        Unit {
            name: "Raketen",
            factor: 60.0,
        },
        Unit {
            name: "Fußballfelder",
            factor: 100.0,
        },
    ];

    let unit = units.choose(&mut rng).unwrap();
    let total_meters = value * unit.factor;

    let reaction = if total_meters < 0.5 {
        "<:Sadge:1434157449497153616>"
    } else if total_meters < 2.0 {
        "<a:PepeLaugh:1434157303426318476>"
    } else if total_meters < 50.0 {
        "<:FeelsOkayMan:1434157605982306475>"
    } else if total_meters < 200.0 {
        "<:poggers:1434154155332604105>"
    } else {
        "<a:Sigma:1434157907771129967>"
    };

    CreateEmbed::new()
        .author(CreateEmbedAuthor::new("Rusty MetalFistBot 7000"))
        .title(format!(
            "Dein Schwanz ist {:.2} {} lang {}",
            value, unit.name, reaction
        ))
        .color(0xFF972C)
        .footer(
            CreateEmbedFooter::new(format!("Requested by {}", command.user.name))
                .icon_url(bot_avatar),
        )
}

pub fn register() -> CreateCommand {
    CreateCommand::new("dick-size").description("how long is your dick")
}
