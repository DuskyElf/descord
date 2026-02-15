use std::collections::HashMap;

use log::{error, info};
use reqwest::Method;

use super::*;
use crate::consts::permissions as perms;
use crate::internals::*;

use crate::models::application_command::{ApplicationCommand, ApplicationCommandOption};

fn map_param_type_to_u32(param_type: &ParamType) -> u32 {
    match param_type {
        ParamType::String => 3,
        ParamType::Int => 4,
        ParamType::User => 6,
        ParamType::Channel => 7,
        _ => 3,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct CommandOption {
    name: String,
    description: String,
    r#type: u32,
    required: bool,
    autocomplete: bool,
}

impl CommandOption {
    fn from_local(
        name: &str,
        description: &str,
        type_: &ParamType,
        required: bool,
        autocomplete: Option<AutoCompleteFn>,
    ) -> Self {
        CommandOption {
            name: name.to_string(),
            description: description.to_string(),
            r#type: map_param_type_to_u32(type_),
            required,
            autocomplete: autocomplete.is_some(),
        }
    }

    fn from_registered(opt: &ApplicationCommandOption) -> Self {
        CommandOption {
            name: opt.name.clone(),
            description: opt.description.clone(),
            r#type: opt.type_,
            required: opt.required.unwrap_or(false),
            autocomplete: opt.autocomplete.unwrap_or(false),
        }
    }
}

pub async fn register_slash_commands(
    commands: Vec<SlashCommand>,
) -> HashMap<String, SlashCommand> {
    let mut slash_commands = HashMap::new();
    let bot_id = match fetch_bot_id().await {
        Ok(id) => id,
        Err(e) => {
            log::error!("Failed to fetch bot id: {}", e);
            return slash_commands;
        }
    };
    let registered_commands = match fetch_application_commands(&bot_id).await {
        Ok(cmds) => cmds,
        Err(e) => {
            log::error!("Failed to fetch application commands: {}", e);
            return slash_commands;
        }
    };

    for local_command in &commands {
        let mut permissions: u64 = 0;
        for permission in &local_command.permissions {
            if let Some(permission) = perms::parse(permission) {
                permissions |= permission;
            } else {
                log::error!("Unknown permission name: {}", permission);
            }
        }

        let local_options = local_command
            .fn_sig
            .iter()
            .enumerate()
            .map(|(i, param_type)| {
                CommandOption::from_local(
                    &local_command.fn_param_names[i],
                    &local_command.fn_param_descriptions[i],
                    param_type,
                    !local_command.optional_params[i],
                    local_command.fn_param_autocomplete[i],
                )
            })
            .collect::<Vec<_>>();
        let options = json::JsonValue::Array(
            local_options
                .iter()
                .map(|opt| {
                    json::object! {
                        "name" => opt.name.clone(),
                        "description" => opt.description.clone(),
                        "type" => opt.r#type,
                        "required" => opt.required,
                        "autocomplete" => opt.autocomplete
                    }
                })
                .collect(),
        );

        // If the command exists in the fetched commands
        if let Some(registered_command) = registered_commands
            .iter()
            .find(|&cmd| cmd.name.as_str() == local_command.name)
        {
            let registered_options = registered_command
                .options
                .as_ref()
                .unwrap_or(&vec![])
                .iter()
                .map(CommandOption::from_registered)
                .collect::<Vec<_>>();

            if local_options != registered_options {
                let _ = request(
                    Method::PATCH,
                    format!("applications/{}/commands/{}", bot_id, registered_command.id).as_str(),
                    Some(
                        json::object! {
                            name: local_command.name.clone(),
                            description: local_command.description.clone(),
                            options: options,
                            default_member_permissions: permissions.to_string(),
                        }
                        .dump()
                        .as_str(),
                    ),
                )
                .await;

                info!(
                    "Updated '{}' slash command, command id: {}",
                    local_command.name, registered_command.id,
                );

                slash_commands.insert(registered_command.id.clone(), local_command.clone());
            } else {
                info!(
                    "No changes detected in '{}' slash command, command id: {}",
                    local_command.name, registered_command.id,
                );

                slash_commands.insert(registered_command.id.clone(), local_command.clone());
            }
        } else {
            // If the command does not exist in the fetched commands, register it
            match request(
                Method::POST,
                format!("applications/{}/commands", bot_id),
                Some(
                    json::object! {
                        name: local_command.name.clone(),
                        description: local_command.description.clone(),
                        options: options,
                        default_member_permissions: permissions.to_string(),
                    }
                    .dump(),
                ),
            )
            .await
            {
                Ok(response) => {
                    if let Ok(text) = response.text().await {
                        if let Ok(json) = json::parse(&text) {
                            if let Some(id) = json["id"].as_str() {
                                let command_id = id.to_string();
                                info!(
                                    "Registered '{}' slash command, command id: {}",
                                    local_command.name, command_id
                                );
                                slash_commands.insert(command_id, local_command.clone());
                            } else {
                                log::error!("Failed to get 'id' from JSON response: {}", text);
                            }
                        } else {
                            log::error!("Failed to parse JSON response: {}", text);
                        }
                    } else {
                        log::error!("Failed to get text from response");
                    }
                }
                Err(e) => {
                    log::error!("Failed to register command: {}", e);
                }
            }
        }
    }

    for registered_command in registered_commands {
        // If the command does not exist in the local commands, remove it
        if !commands
            .iter()
            .any(|cmd| cmd.name == registered_command.name)
        {
            let _ = request(
                Method::DELETE,
                format!("applications/{}/commands/{}", bot_id, registered_command.id).as_str(),
                None,
            )
            .await;

            info!(
                "Removed slash command '{}', command id: {}",
                registered_command.name, registered_command.id
            );
        }
    }

    slash_commands
}
