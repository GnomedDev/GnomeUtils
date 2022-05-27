#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)] // I honestly cannot be bothered to document rn
#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation, clippy::cast_sign_loss)]

pub use poise;
pub use poise::serenity_prelude as serenity;

pub mod analytics;
pub mod logging;
pub mod errors;
mod macros;
mod traits;
mod looper;

pub use traits::{PoiseContextExt, OptionGettext, OptionTryUnwrap};
pub use looper::Looper;

#[allow(clippy::unreadable_literal)]
pub const RED: u32 = 0xff0000;

pub type Context<'a, D> = poise::Context<'a, D, anyhow::Error>;
pub type FrameworkContext<'a, D> = poise::FrameworkContext<'a, D, anyhow::Error>;

#[derive(Debug)]
pub struct GnomeData {
    pub pool: sqlx::PgPool,
    pub main_server_invite: String,
    pub error_webhook: serenity::Webhook,
    pub system_info: parking_lot::Mutex<sysinfo::System>,
    pub translations: std::collections::HashMap<String, gettext::Catalog>,
}
