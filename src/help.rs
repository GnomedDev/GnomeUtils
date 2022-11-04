use std::fmt::Write as _;

use anyhow::Result;
use indexmap::IndexMap;

use crate::{Command, GnomeData, Context, require, ApplicationContext, PoiseContextExt, serenity};

enum HelpCommandMode<'a, D: AsRef<GnomeData>> {
    Root,
    Group(&'a Command<D>),
    Command(&'a Command<D>),
}

fn get_command_mapping<D: AsRef<GnomeData>>(commands: &[Command<D>]) -> IndexMap<&str, Vec<&Command<D>>> {
    let mut mapping = IndexMap::new();

    for command in commands {
        if !command.hide_in_help {
            let commands = mapping
                .entry(command.category.unwrap_or("Uncategoried"))
                .or_insert_with(Vec::new);

            commands.push(command);
        }
    }

    mapping
}

fn format_params(command: &Command<impl AsRef<GnomeData>>) -> String {
    command.parameters.iter().map(|p| {
        if p.required {
            format!("<{}> ", p.name)
        } else {
            format!("[{}] ", p.name)
        }
    }).collect()
}

fn show_group_description(group: &IndexMap<&str, Vec<&Command<impl AsRef<GnomeData>>>>) -> String {
    group.iter().map(|(category, commands)| {
        format!("**__{category}__**\n{}\n", commands.iter().map(|c| {
            let params = format_params(c);
            if params.is_empty() {
                format!("`{}`: {}\n", c.qualified_name, c.description.as_ref().unwrap())
            } else {
                format!("`{} {params}`: {}\n", c.qualified_name, c.description.as_ref().unwrap())
            }
        }).collect::<String>()
    )}).collect::<String>()
}


pub async fn command(ctx: Context<'_, impl AsRef<GnomeData> + Send + Sync>, command: Option<&str>, neutral_colour: u32) -> Result<()> {
    let framework_options = ctx.framework().options();
    let commands = &framework_options.commands;

    let remaining_args: String;
    let mode = match command {
        None => HelpCommandMode::Root,
        Some(command) => {
            let mut subcommand_iterator = command.split(' ');

            let top_level_command = subcommand_iterator.next().unwrap();
            let (mut command_obj, _, _) = require!(poise::find_command(commands, top_level_command, true, &mut Vec::new()), {
                ctx.say(ctx.gettext("No command called {} found!").replace("{}", top_level_command)).await?;
                Ok(())
            });

            remaining_args = subcommand_iterator.collect();
            if !remaining_args.is_empty() {
                (command_obj, _, _) = require!(poise::find_command(&command_obj.subcommands, &remaining_args, true, &mut Vec::new()), {
                    ctx.say(ctx
                        .gettext("The group {group_name} does not have a subcommand called {subcommand_name}!")
                        .replace("{subcommand_name}", &remaining_args).replace("{group_name}", &command_obj.name)
                    ).await.map(drop).map_err(Into::into)
                });
            };

            if command_obj.owners_only && !framework_options.owners.contains(&ctx.author().id) {
                ctx.say(ctx.gettext("This command is only available to the bot owner!")).await?;
                return Ok(())
            }

            if command_obj.subcommands.is_empty() {
                HelpCommandMode::Command(command_obj)
            } else {
                HelpCommandMode::Group(command_obj)
            }
        }
    };

    ctx.send(poise::CreateReply::default().embed(serenity::CreateEmbed::default()
        .title(ctx.gettext("{command_name} Help!").replace("{command_name}", &match &mode {
            HelpCommandMode::Root => ctx.discord().cache.current_user().name.clone(),
            HelpCommandMode::Group(c) | HelpCommandMode::Command(c) => format!("`{}`", c.qualified_name) 
        }))
        .description(match &mode {
            HelpCommandMode::Root => show_group_description(&get_command_mapping(commands)),
            HelpCommandMode::Command(command_obj) => {
                let mut msg = format!("{}\n```/{} {}```\n",
                    command_obj.description.as_deref().unwrap_or_else(|| ctx.gettext("Command description not found!")),
                    command_obj.qualified_name, format_params(command_obj),
                );

                if !command_obj.parameters.is_empty() {
                    msg.push_str(ctx.gettext("__**Parameter Descriptions**__\n"));
                    command_obj.parameters.iter().for_each(|p|
                        writeln!(msg, "`{}`: {}", p.name, p.description.as_deref().unwrap_or_else(|| ctx.gettext("no description"))).unwrap()
                    );
                };

                msg
            },
            HelpCommandMode::Group(group) => show_group_description(&{
                let mut map: IndexMap<&str, Vec<&Command<_>>> = IndexMap::new();
                map.insert(&group.qualified_name, group.subcommands.iter().collect());
                map
            }),
        })
        .colour(neutral_colour)
        .author(serenity::CreateEmbedAuthor::new(ctx.author().name.clone()).icon_url(ctx.author().face()))
        .footer(serenity::CreateEmbedFooter::new(match mode {
            HelpCommandMode::Group(c) => ctx
                .gettext("Use `/help {command_name} [command]` for more info on a command")
                .replace("{command_name}", &c.qualified_name),
            HelpCommandMode::Command(_) |HelpCommandMode::Root => ctx
                .gettext("Use `/help [command]` for more info on a command")
                .to_string()
        }))
    )).await?;

    Ok(())
}

#[allow(clippy::unused_async)]
pub async fn autocomplete(ctx: ApplicationContext<'_, impl AsRef<GnomeData>>, searching: &str) -> Vec<String> {
    fn flatten_commands(commands: &[Command<impl AsRef<GnomeData>>], searching: &str) -> Vec<String> {
        let mut result = Vec::new();

        for command in commands {
            if command.owners_only || command.hide_in_help {
                continue
            }

            if command.subcommands.is_empty() {
                if command.qualified_name.starts_with(searching) {
                    result.push(command.qualified_name.clone());
                }
            } else {
                result.extend(flatten_commands(&command.subcommands, searching));
            }
        }

        result
    }

    let mut result: Vec<String> = flatten_commands(&ctx.framework.options().commands, searching);
    result.sort_by_key(|a| strsim::levenshtein(a, searching));
    result
}
