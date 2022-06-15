#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)] // I honestly cannot be bothered to document rn
#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation, clippy::cast_sign_loss)]

#[cfg(feature = "poise")]
pub use poise::{self, serenity_prelude as serenity};

#[cfg(feature = "logging")] pub mod logging;
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

#[cfg(feature = "poise")]
pub type Framework<D> = poise::Framework<D, anyhow::Error>;
#[cfg(feature = "poise")]
pub type Context<'a, D> = poise::Context<'a, D, anyhow::Error>;
#[cfg(feature = "poise")]
pub type FrameworkContext<'a, D> = poise::FrameworkContext<'a, D, anyhow::Error>;
#[cfg(feature = "poise")]
pub async fn framework_to_context<D>(framework: &Framework<D>, bot_id: serenity::UserId) -> FrameworkContext<'_, D> {
    FrameworkContext {
        bot_id,
        options: framework.options(),
        user_data: framework.user_data().await,
        shard_manager: framework.shard_manager()
    }
}


#[derive(Debug)]
pub struct GnomeData {
    pub main_server_invite: String,
    #[cfg(feature = "error_handling")] pub pool: sqlx::PgPool,
    #[cfg(feature = "error_handling")] pub error_webhook: serenity::Webhook,
    #[cfg(feature = "error_handling")] pub system_info: parking_lot::Mutex<sysinfo::System>,
    #[cfg(feature = "i18n")] pub translations: std::collections::HashMap<String, gettext::Catalog>,
}
