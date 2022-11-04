use anyhow::Result;

#[cfg(feature = "poise")]
use crate::{serenity, GnomeData};

#[cfg(feature = "i18n")]
pub trait OptionGettext<'a> {
    fn gettext(self, translate: &'a str) -> &'a str;
}

#[cfg(feature = "i18n")]
impl<'a> OptionGettext<'a> for Option<&'a gettext::Catalog> {
    fn gettext(self, translate: &'a str) -> &'a str {
        self.map_or(translate, |c| c.gettext(translate))
    }
}

pub trait OptionTryUnwrap<T> {
    fn try_unwrap(self) -> Result<T>;
}

impl<T> OptionTryUnwrap<T> for Option<T> {
    #[track_caller]
    fn try_unwrap(self) -> Result<T> {
        match self {
            Some(v) => Ok(v),
            None => Err({
                let location = std::panic::Location::caller();
                anyhow::anyhow!("Unexpected None value on line {} in {}", location.line(), location.file())
            })
        }
    }
}

#[cfg(feature = "poise")]
#[serenity::async_trait]
pub trait PoiseContextExt {
    #[cfg(feature = "i18n")]
    fn gettext<'a>(&'a self, translate: &'a str) -> &'a str;
    #[cfg(not(feature = "i18n"))]
    fn gettext<'a>(&self, translate: &'a str) -> &'a str;

    #[cfg(feature = "i18n")]
    fn current_catalog(&self) -> Option<&gettext::Catalog>;
    #[cfg(feature = "error_handling")]
    async fn send_error(&self, error: &str, fix: Option<&str>) -> Result<Option<poise::ReplyHandle<'_>>>;

    async fn author_permissions(&self) -> Result<serenity::Permissions>;
}

#[cfg(feature = "poise")]
#[serenity::async_trait]
impl<D: AsRef<GnomeData> + Send + Sync, E: Send + Sync> PoiseContextExt for poise::Context<'_, D, E> {
    #[cfg(feature = "i18n")]
    fn gettext<'a>(&'a self, translate: &'a str) -> &'a str {
        self.current_catalog().gettext(translate)
    }

    #[cfg(not(feature = "i18n"))]
    fn gettext<'a>(&self, translate: &'a str) -> &'a str {
        translate
    }

    #[cfg(feature = "i18n")]
    fn current_catalog(&self) -> Option<&gettext::Catalog> {
        if let poise::Context::Application(ctx) = self {
            if let poise::CommandOrAutocompleteInteraction::Command(interaction) = ctx.interaction {
                return ctx.data.as_ref().translations.get(match interaction.locale.as_str() {
                    "ko" => "ko-KR",
                    "pt-BR" => "pt",
                    l => l
                })
            }
        };

        None
    }

    async fn author_permissions(&self) -> Result<serenity::Permissions> {
        let ctx_discord = self.discord();

        match ctx_discord.cache.channel(self.channel_id()).try_unwrap()? {
            serenity::Channel::Guild(channel) => {
                let member = channel.guild_id.member(ctx_discord, self.author()).await?;
                let guild = channel.guild(&ctx_discord.cache).try_unwrap()?;

                Ok(guild.user_permissions_in(&channel, &member)?)
            }
            _ => {
                Ok(((serenity::Permissions::from_bits_truncate(0b111_1100_1000_0000_0000_0111_1111_1000_0100_0000)
                    | serenity::Permissions::SEND_MESSAGES)
                    - serenity::Permissions::SEND_TTS_MESSAGES)
                    - serenity::Permissions::MANAGE_MESSAGES)
            }
        }
    }

    #[cfg(feature = "error_handling")]
    async fn send_error(&self, error: &str, fix: Option<&str>) -> Result<Option<poise::ReplyHandle<'_>>> {
        let author = self.author();
        let ctx_discord = self.discord();

        let m;
        let (name, avatar_url) = match self.channel_id().to_channel(ctx_discord).await? {
            serenity::Channel::Guild(channel) => {
                let permissions = channel.permissions_for_user(ctx_discord, ctx_discord.cache.current_user().id)?;

                if !permissions.send_messages() {
                    return Ok(None);
                };

                if !permissions.embed_links() {
                    return self.send(poise::CreateReply::default()
                        .ephemeral(true)
                        .content("An Error Occurred! Please give me embed links permissions so I can tell you more!")
                    ).await.map(Some).map_err(Into::into)
                };

                match channel.guild_id.member(ctx_discord, author.id).await {
                    Ok(member) => {
                        m = member;
                        (m.display_name(), m.face())
                    },
                    Err(_) => (std::borrow::Cow::Borrowed(&author.name), author.face()),
                }
            }
            serenity::Channel::Private(_) => (std::borrow::Cow::Borrowed(&author.name), author.face()),
            _ => unreachable!(),
        };

        match self.send(poise::CreateReply::default()
            .ephemeral(true)
            .embed(serenity::CreateEmbed::default()
                .colour(crate::RED)
                .title("An Error Occurred!")
                .author(serenity::CreateEmbedAuthor::new(name.into_owned()).icon_url(avatar_url))
                .description(format!(
                    "Sorry but {}, to fix this, please {error}!",
                    fix.unwrap_or("get in contact with us via the support server"),
                ))
                .footer(serenity::CreateEmbedFooter::new(format!(
                    "Support Server: {}", self.data().as_ref().main_server_invite
                )))
            )
        ).await {
            Ok(handle) => Ok(Some(handle)),
            Err(_) => Ok(None)
        }
    }
}
