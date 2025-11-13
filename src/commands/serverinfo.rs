use crate::BotData;
use serenity::all::CreateCommand;
use serenity::{
    builder::{CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter},
    model::prelude::*,
    prelude::*,
};

pub async fn run(ctx: &Context, command: &CommandInteraction) -> CreateEmbed {
    let data = ctx.data.read().await;
    let bot_user = data.get::<BotData>().unwrap();

    let guild = ctx.cache.guild(command.guild_id.unwrap()).unwrap().clone();

    let server_icon_url = guild.icon_url().unwrap_or_default();

    let owner = guild.member(&ctx.http, guild.owner_id).await.ok();
    let owner_name = if let Some(ref o) = owner {
        let user = &o.user;
        user.global_name.as_ref().unwrap_or(&user.name).to_string()
    } else {
        "Unknown".to_string()
    };

    let mut online_total_members = 0;
    let mut online_bot_members = 0;

    for member in guild.members.values() {
        if let Some(presence) = guild.presences.get(&member.user.id) {
            match presence.status {
                serenity::model::user::OnlineStatus::Online
                | serenity::model::user::OnlineStatus::Idle
                | serenity::model::user::OnlineStatus::DoNotDisturb => {
                    online_total_members += 1;
                    if member.user.bot {
                        online_bot_members += 1;
                    }
                }
                _ => {}
            }
        }
    }

    let emoji_count = guild.emojis.len();

    // Verification Level
    let verification_level = match guild.verification_level {
        VerificationLevel::None => "None",
        VerificationLevel::Low => "Low",
        VerificationLevel::Medium => "Medium",
        VerificationLevel::High => "High",
        VerificationLevel::Higher => "Very High",
        _ => "Unknown",
    };

    //  let system_channel_id = guild.system_channel_id;//.unwrap().name(ctx.http.clone()).await.unwrap_or_else(|_| "unknown".to_string());

    

    CreateEmbed::new()
        .color(0xffcc00)
        .author(
            CreateEmbedAuthor::new("Rusty MetalFistBot 7000 | Serverinfo")
                .icon_url(bot_user.bot_pfp_url.clone()),
        )
        .title(&guild.name)
        .thumbnail(server_icon_url)
        .field("Owner", owner_name, true)
        .field("Member", guild.member_count.to_string(), true)
        .field("Server ID", guild.id.to_string(), true)
        .field("Verification level", verification_level, true)
        .field("Channels", guild.channels.len().to_string(), true)
        .field(
            "Online",
            format!("{} ({} bots)", online_total_members, online_bot_members),
            true,
        )
        .field("Roles", guild.roles.len().to_string(), true)
        .field("Emojis", emoji_count.to_string(), true)
        .field(
            "System channel",
            format!("<#{}>", guild.system_channel_id.unwrap()),
            true,
        )
        .timestamp(guild.id.created_at())
        .footer(CreateEmbedFooter::new("Server created on"))
}

pub fn register() -> CreateCommand {
    CreateCommand::new("serverinfo").description("Get some basic info about the current server")
}
