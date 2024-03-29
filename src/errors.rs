//! Webhook error handler for poise
//!
//! Requirements:
//! - Must have a table with the following schema:
//! 
//! ```sql
//! CREATE TABLE errors (
//!     traceback   text    PRIMARY KEY,
//!     message_id  bigint  NOT NULL,
//!     occurrences int     DEFAULT 1
//! );
//! ```

use std::borrow::Cow;
#[cfg(feature = "songbird")]
use std::sync::Arc;

use anyhow::{Error, Result};
use sha2::Digest;
use sysinfo::SystemExt;
use tracing::error;

use poise::serenity_prelude as serenity;

#[cfg(feature = "songbird")]
use crate::{Framework, framework_to_context};
use crate::{GnomeData, require, FrameworkContext, PoiseContextExt, Context};

const VIEW_TRACEBACK_CUSTOM_ID: &str = "error::traceback::view";


#[derive(sqlx::FromRow)]
struct ErrorRowWithOccurrences {
    pub message_id: i64,
    pub occurrences: i32,
}

#[derive(sqlx::FromRow)]
struct ErrorRow {
    pub message_id: i64
}

#[derive(sqlx::FromRow)]
struct TracebackRow {
    pub traceback: String
}

#[must_use]
pub const fn blank_field() -> (&'static str, Cow<'static, str>, bool) {
    ("\u{200B}", Cow::Borrowed("\u{200B}"), true)
}

fn hash(data: &[u8]) -> Vec<u8> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    Vec::from(&*hasher.finalize())
}

pub async fn handle_unexpected<'a>(
    ctx: &serenity::Context,
    poise_context: FrameworkContext<'_, impl AsRef<GnomeData>>,
    event: &'a str,
    error: Error,
    extra_fields: impl IntoIterator<Item = (&str, Cow<'a, str>, bool)>,
    author_name: Option<String>,
    icon_url: Option<String>
) -> Result<()> {
    let data = poise_context.user_data.as_ref();
    let error_webhook = &data.error_webhook;

    let traceback = format!("{:?}", error);

    let traceback_hash = hash(traceback.as_bytes());
    let mut conn = data.pool.acquire().await?;

    if let Some(ErrorRowWithOccurrences{message_id, occurrences}) = sqlx::query_as("
        UPDATE errors SET occurrences = occurrences + 1
        WHERE traceback_hash = $1
        RETURNING message_id, occurrences
    ").bind(traceback_hash.clone()).fetch_optional(&mut conn).await? {
        let message_id = serenity::model::id::MessageId(message_id as u64);
        let mut message = error_webhook.get_message(&ctx.http, message_id).await?;
        let embed = &mut message.embeds[0];

        let footer = format!("This error has occurred {} times!", occurrences);
        embed.footer.as_mut().unwrap().text = footer;

        error_webhook.edit_message(ctx, message_id,  |m| {m.embeds(vec![
            serenity::json::prelude::to_value(embed).unwrap()
        ])}).await?;
    } else {
        let short_error = {
            let mut long_err = error.to_string();

            // Avoid char boundary panics with utf8 chars
            let mut new_len = 256;
            while !long_err.is_char_boundary(new_len) {
                new_len -= 1;
            }

            long_err.truncate(new_len);
            long_err
        };

        let (cpu_usage, mem_usage) ={
            let mut system = data.system_info.lock();
            system.refresh_specifics(sysinfo::RefreshKind::new()
                .with_cpu(sysinfo::CpuRefreshKind::new().with_cpu_usage())
                .with_processes(sysinfo::ProcessRefreshKind::new())
                .with_memory()
            );

            (
                system.load_average().five.to_string(),
                (system.used_memory() / 1024).to_string()
            )
        };

        let before_fields = [
            ("Event", Cow::Borrowed(event), true),
            ("Bot User", Cow::Owned(ctx.cache.current_user_field(|u| u.name.clone())), true),
            blank_field(),
        ];

        let shard_count = poise_context.shard_manager.lock().await.shards_instantiated().await.len();
        let after_fields = [
            ("CPU Usage (5 minutes)", Cow::Owned(cpu_usage), true),
            ("System Memory Usage", Cow::Owned(mem_usage), true),
            ("Shard Count", Cow::Owned(shard_count.to_string()), true),
        ];

        let embed = serenity::model::channel::Embed::fake(|e| {
            before_fields.into_iter()
                .chain(extra_fields)
                .chain(after_fields)
                .for_each(|(title, mut value, inline)| {
                    if value != "\u{200B}" {
                        value = Cow::Owned(format!("`{value}`"));
                    };

                    e.field(title, &*value, inline);
                });

            if let Some(author_name) = author_name {
                e.author(|a| {
                    if let Some(icon_url) = icon_url {
                        a.icon_url(icon_url);
                    }
                    a.name(author_name)
                });
            }

            e.footer(|f| f.text("This error has occurred 1 time!"));
            e.title(short_error);
            e.colour(crate::RED)
        });

        let message = error_webhook.execute(&ctx.http, true, |b| {b
            .embeds(vec![embed])
            .components(|c| c.create_action_row(|a| a.create_button(|b| {b
                .label("View Traceback")
                .custom_id(VIEW_TRACEBACK_CUSTOM_ID)
                .style(serenity::ButtonStyle::Danger)
            })))
        }).await?.unwrap();

        let ErrorRow{message_id} = sqlx::query_as("
            INSERT INTO errors(traceback_hash, traceback, message_id)
            VALUES($1, $2, $3)

            ON CONFLICT (traceback_hash)
            DO UPDATE SET occurrences = errors.occurrences + 1
            RETURNING errors.message_id
        ",).bind(traceback_hash).bind(traceback).bind(message.id.0 as i64).fetch_one(&mut conn).await?;

        if message.id.0 != (message_id as u64) {
            error_webhook.delete_message(&ctx.http, message.id).await?;
        }
    };

    Ok(())
}

pub async fn handle_unexpected_default(ctx: &serenity::Context, poise_context: FrameworkContext<'_, impl AsRef<GnomeData>>, name: &str, result: Result<()>) -> Result<()> {
    let error = require!(result.err(), Ok(()));

    handle_unexpected(
        ctx, poise_context,
        name, error, [],
        None, None
    ).await
}


// Listener Handlers
pub async fn handle_message(ctx: &serenity::Context, poise_context: FrameworkContext<'_, impl AsRef<GnomeData>>, message: &serenity::Message, result: Result<impl Send + Sync>) -> Result<()> {
    let error = require!(result.err(), Ok(()));

    let mut extra_fields = Vec::with_capacity(3);
    if let Some(guild_id) = message.guild_id {
        if let Some(guild_name) = ctx.cache.guild_field(guild_id, |g| g.name.clone()) {
            extra_fields.push(("Guild", Cow::Owned(guild_name), true));
        }

        extra_fields.push(("Guild ID", Cow::Owned(guild_id.0.to_string()), true));
    }

    extra_fields.push(("Channel Type", Cow::Borrowed(channel_type(&message.channel_id.to_channel(&ctx).await?)), true));
    handle_unexpected(
        ctx, poise_context,
        "MessageCreate", error, extra_fields,
        Some(message.author.name.clone()), Some(message.author.face())
    ).await
}

pub async fn handle_member(ctx: &serenity::Context, poise_context: FrameworkContext<'_, impl AsRef<GnomeData>>, member: &serenity::Member, result: Result<(), impl Into<Error>>) -> Result<()> {
    let error = require!(result.err(), Ok(())).into();

    let extra_fields = [
        ("Guild", Cow::Owned(member.guild_id.to_string()), true),
        ("Guild ID", Cow::Owned(member.guild_id.to_string()), true),
        ("User ID", Cow::Owned(member.user.id.0.to_string()), true),
    ];

    handle_unexpected(
        ctx, poise_context,
        "GuildMemberAdd", error, extra_fields,
        None, None
    ).await
}

pub async fn handle_guild(name: &str, ctx: &serenity::Context, poise_context: FrameworkContext<'_, impl AsRef<GnomeData>>, guild: Option<&serenity::Guild>, result: Result<()>) -> Result<()> {
    let error = require!(result.err(), Ok(()));

    handle_unexpected(
        ctx, poise_context,
        name, error, [],
        guild.as_ref().map(|g| g.name.clone()),
        guild.and_then(serenity::Guild::icon_url),
    ).await
}


// Command Error handlers
async fn handle_cooldown<D: AsRef<GnomeData> + Send + Sync>(ctx: Context<'_, D>, remaining_cooldown: std::time::Duration) -> Result<(), Error> {
    let cooldown_response = ctx.send_error(
        &ctx.gettext("{command_name} is on cooldown").replace("{command_name}", &ctx.command().name),
        Some(&ctx.gettext("try again in {} seconds").replace("{}", &format!("{:.1}", remaining_cooldown.as_secs_f32())))
    ).await?;

    if let poise::Context::Prefix(ctx) = ctx {
        if let Some(cooldown_response) = cooldown_response {
            let ctx_discord = ctx.discord;
            tokio::time::sleep(remaining_cooldown).await;

            let error_message = cooldown_response.into_message().await?;
            error_message.delete(ctx_discord).await?;

            let bot_user_id = ctx_discord.cache.current_user_id();
            let channel = error_message.channel(ctx_discord).await?.guild().unwrap();

            if channel.permissions_for_user(ctx_discord, bot_user_id)?.manage_messages() {
                ctx.msg.delete(ctx_discord).await?;
            }
        }
    };

    Ok(())
}

async fn handle_argparse<D: AsRef<GnomeData> + Send + Sync>(ctx: Context<'_, D>, error: Box<dyn std::error::Error + Send + Sync>, input: Option<String>) -> Result<(), Error> {
    let fix = None;
    let mut reason = None;

    if error.is::<serenity::MemberParseError>() {
        reason = Some(ctx.gettext("I cannot find the member: `{}`"));
    } else if error.is::<serenity::GuildParseError>() {
        reason = Some(ctx.gettext("I cannot find the server: `{}`"));
    } else if error.is::<serenity::GuildChannelParseError>() {
        reason = Some(ctx.gettext("I cannot find the channel: `{}`"));
    } else if error.is::<std::num::ParseIntError>() {
        reason = Some(ctx.gettext("I cannot convert `{}` to a number"));
    } else if error.is::<std::str::ParseBoolError>() {
        reason = Some(ctx.gettext("I cannot convert `{}` to True/False"));
    }

    ctx.send_error(
        reason.map(|r| r.replace("{}", &input.unwrap()).replace('`', "")).as_deref().unwrap_or("you typed the command wrong"),
        Some(&fix.unwrap_or_else(|| ctx
                .gettext("check out `/help {command}`")
                .replace("{command}", &ctx.command().qualified_name)))
    ).await?;

    Ok(())
}


const fn channel_type(channel: &serenity::Channel) -> &'static str {
    use self::serenity::{Channel, ChannelType};

    match channel {
        Channel::Guild(channel)  => match channel.kind {
            ChannelType::Text | ChannelType::News => "Text Channel",   
            ChannelType::Voice => "Voice Channel",
            ChannelType::NewsThread => "News Thread Channel",
            ChannelType::PublicThread => "Public Thread Channel",
            ChannelType::PrivateThread => "Private Thread Channel",
            _ => "Unknown Channel Type",
        },
        Channel::Private(_) => "Private Channel",
        Channel::Category(_) => "Category Channel??",
        _ => "Unknown Channel Type",
    }
}

pub async fn handle<D: AsRef<GnomeData> + Send + Sync>(error: poise::FrameworkError<'_, D, Error>) -> Result<(), Error> {
    match error {
        poise::FrameworkError::DynamicPrefix { error } => error!("Error in dynamic_prefix: {:?}", error),
        poise::FrameworkError::Command { error, ctx } => {
            let author = ctx.author();
            let command = ctx.command();

            let mut extra_fields = vec![
                ("Command", Cow::Borrowed(&*command.name), true),
                ("Slash Command", Cow::Owned(matches!(ctx, poise::Context::Application(..)).to_string()), true),
                ("Channel Type", Cow::Borrowed(channel_type(&ctx.channel_id().to_channel(ctx.discord()).await?)), true),
            ];

            if let Some(guild) = ctx.guild() {
                extra_fields.extend([
                    ("Guild", Cow::Owned(guild.name), true),
                    ("Guild ID", Cow::Owned(guild.id.0.to_string()), true),
                    blank_field()
                ]);
            }

            handle_unexpected(
                ctx.discord(), ctx.framework(),
                "command", error, extra_fields,
                Some(author.name.clone()), Some(author.face())
            ).await?;

            ctx.send_error("an unknown error occurred", None).await?;
        }
        poise::FrameworkError::ArgumentParse { error, ctx, input } => handle_argparse(ctx, error, input).await?,
        poise::FrameworkError::CooldownHit { remaining_cooldown, ctx } => handle_cooldown(ctx, remaining_cooldown).await?,
        poise::FrameworkError::MissingBotPermissions{missing_permissions, ctx} => {
            ctx.send_error(
                &ctx.gettext("I cannot run `{command}` as I am missing permissions").replace("{command}", &ctx.command().name),
                Some(&ctx.gettext("give me: {}").replace("{}", &missing_permissions.get_permission_names().join(", ")))
            ).await?;
        },
        poise::FrameworkError::MissingUserPermissions{missing_permissions, ctx} => {
            ctx.send_error(
                ctx.gettext("you cannot run this command"),
                missing_permissions.map(|missing_permissions| (ctx
                    .gettext("ask an administrator for the following permissions: {}")
                    .replace("{}", &missing_permissions.get_permission_names().join(", "))
                )).as_deref()
            ).await?;
        },

        poise::FrameworkError::Setup { error } => panic!("{:#?}", error),
        poise::FrameworkError::CommandCheckFailed { error, ctx } => {
            if let Some(error) = error {
                error!("Premium Check Error: {:?}", error);
                ctx.send_error(ctx.gettext("an unknown error occurred during the premium check"), None).await?;
            }
        },

        poise::FrameworkError::Listener{..} => unreachable!("Listener error, but no listener???"),
        poise::FrameworkError::CommandStructureMismatch {description: _, ctx: _} |
        poise::FrameworkError::DmOnly {ctx: _ } |
        poise::FrameworkError::NsfwOnly {ctx: _} | 
        poise::FrameworkError::NotAnOwner{ctx: _} => {},
        poise::FrameworkError::GuildOnly {ctx} => {
            ctx.send_error(
                &ctx.gettext("{command_name} cannot be used in private messages").replace("{command_name}", &ctx.command().qualified_name),
                Some(&ctx.discord().cache.current_user_field(|b| ctx
                    .gettext("try running it on a server with {bot_name} in")
                    .replace("{bot_name}", &b.name)
                ))
            ).await?;
        },
        poise::FrameworkError::__NonExhaustive => unreachable!(),
    }

    Ok(())
}


pub async fn interaction_create(ctx: serenity::Context, interaction: serenity::Interaction, framework: FrameworkContext<'_, impl AsRef<GnomeData>>) {
    if let serenity::Interaction::MessageComponent(interaction) = interaction {
        if interaction.data.custom_id == VIEW_TRACEBACK_CUSTOM_ID {
            handle_unexpected_default(&ctx, framework, "InteractionCreate",
                handle_traceback_button(&ctx, framework.user_data.as_ref(), interaction).await
            ).await.unwrap_or_else(|err| error!("on_error: {:?}", err));
        };
    }
}

pub async fn handle_traceback_button(ctx: &serenity::Context, data: &GnomeData, interaction: serenity::MessageComponentInteraction) -> Result<(), Error> {
    let row: Option<TracebackRow> = sqlx::query_as("SELECT traceback FROM errors WHERE message_id = $1")
        .bind(interaction.message.id.0 as i64)
        .fetch_optional(&data.pool)
        .await?;

    interaction.create_interaction_response(&ctx.http, |r| {r
        .kind(serenity::InteractionResponseType::ChannelMessageWithSource)
        .interaction_response_data(move |d| {
            d.ephemeral(true);

            if let Some(TracebackRow{traceback}) = row {
                d.files([serenity::AttachmentType::Bytes {
                    data: Cow::Owned(traceback.into_bytes()),
                    filename: String::from("traceback.txt")
                }])
            } else {
                d.content("No traceback found.")
            }
        })
    }).await?;

    Ok(())
}


#[cfg(feature = "songbird")]
struct TrackErrorHandler<D, Iter: IntoIterator<Item = (&'static str, Cow<'static, str>, bool)>> {
    ctx: serenity::Context,
    framework: Arc<Framework<D>>,
    extra_fields: Iter,
    author_name: String,
    icon_url: String,
}

#[cfg(feature = "songbird")]
#[async_trait::async_trait]
impl<D, Iter> songbird::EventHandler for TrackErrorHandler<D, Iter>
where
    Iter: IntoIterator<Item = (&'static str, Cow<'static, str>, bool)> + Clone + Send + Sync,
    D: AsRef<GnomeData> + Send + Sync,
{
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        if let songbird::EventContext::Track([(state, _)]) = ctx {
            if let songbird::tracks::PlayMode::Errored(error) = state.playing.clone() {
                let framework_context = framework_to_context(&self.framework, self.ctx.cache.current_user_id()).await;
                let author_name = Some(self.author_name.clone());
                let icon_url = Some(self.icon_url.clone());

                let result = handle_unexpected(
                    &self.ctx, framework_context,
                    "TrackError", error.into(),
                    self.extra_fields.clone(), author_name, icon_url
                ).await;

                if let Err(err_err) = result {
                    tracing::error!("Songbird unhandled track error: {}", err_err);
                }
            }
        }

        Some(songbird::Event::Cancel)
    }
}

#[cfg(feature = "songbird")]
/// Registers a track to be handled by the error handler, arguments other than the
/// track are passed to [`handle_unexpected`] if the track errors.
pub fn handle_track<Iter, D>(
    ctx: serenity::Context,
    framework: Arc<Framework<D>>,
    extra_fields: Iter,
    author_name: String,
    icon_url: String,

    track: &songbird::tracks::TrackHandle
) -> Result<(), songbird::error::ControlError>
where
    Iter: IntoIterator<Item = (&'static str, Cow<'static, str>, bool)> + Clone + Send + Sync + 'static,
    D: AsRef<GnomeData> + Send + Sync + 'static,
{
    track.add_event(
        songbird::Event::Track(songbird::TrackEvent::Error),
        TrackErrorHandler {ctx, framework, extra_fields, author_name, icon_url}
    )
}
