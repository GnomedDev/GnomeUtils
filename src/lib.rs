#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)] // I honestly cannot be bothered to document rn
#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation, clippy::cast_sign_loss)]

#[cfg(feature = "poise")]
pub use poise::{self, serenity_prelude as serenity};

#[cfg(feature = "logging")] pub mod logging;
#[cfg(feature = "help_command")] pub mod help;
#[cfg(feature = "analytics")] pub mod analytics;
#[cfg(feature = "bot_list")] mod bot_list_updater;
#[cfg(feature = "error_handling")] pub mod errors;
mod macros;
mod traits;
mod looper;

#[cfg(feature = "bot_list")] pub use bot_list_updater::{BotListUpdater, BotListTokens};
#[cfg(feature = "poise")] pub use traits::PoiseContextExt;
#[cfg(feature = "i18n")] pub use traits::OptionGettext;
pub use traits::OptionTryUnwrap;
pub use looper::Looper;

#[allow(clippy::unreadable_literal)]
pub const RED: u32 = 0xff0000;

#[cfg(feature="poise")]
mod poise_specific {
    pub type Command<D> = poise::Command<D, anyhow::Error>;
    pub type Framework<D> = poise::Framework<D, anyhow::Error>;
    pub type Context<'a, D> = poise::Context<'a, D, anyhow::Error>;
    pub type FrameworkContext<'a, D> = poise::FrameworkContext<'a, D, anyhow::Error>;
    pub type ApplicationContext<'a, D> = poise::ApplicationContext<'a, D, anyhow::Error>;
    pub async fn framework_to_context<D>(framework: &Framework<D>) -> FrameworkContext<'_, D> {
        let user_data = framework.user_data().await;
        let bot_id = framework.bot_id().await;
    
        FrameworkContext {
            bot_id, user_data,
            options: framework.options(),
            shard_manager: framework.shard_manager()
        }
    }
}

#[cfg(feature="poise")]
pub use poise_specific::*;

#[derive(Debug)]
pub struct GnomeData {
    pub main_server_invite: String,
    #[cfg(feature = "error_handling")] pub pool: sqlx::PgPool,
    #[cfg(feature = "error_handling")] pub error_webhook: serenity::Webhook,
    #[cfg(feature = "error_handling")] pub system_info: parking_lot::Mutex<sysinfo::System>,
    #[cfg(feature = "i18n")] pub translations: std::collections::HashMap<String, gettext::Catalog>,
}
