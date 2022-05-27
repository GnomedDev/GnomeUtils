#[macro_export]
macro_rules! require {
    ($to_check:expr) => {
        require!($to_check, ())
    };
    ($to_check:expr, $ret:expr) => {
        if let Some(to_check) = $to_check {
            to_check
        } else {
            return $ret;
        }
    };
}

#[macro_export]
macro_rules! require_guild {
    ($ctx:expr) => {
        $crate::require!($ctx.guild(), {
            ::tracing::warn!("Guild {} not cached in {} command!", $ctx.guild_id().unwrap(), $ctx.command().qualified_name);
            Ok(())
        })
    };
}
