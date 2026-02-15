use std::collections::HashMap;

mod commands;
mod components;
mod events;
mod slash_commands;

pub use commands::*;
pub use components::*;
pub use events::*;
pub use slash_commands::*;

use crate::consts::events::Event;
use crate::models::channel::Channel;
use crate::models::deleted_message_response::DeletedMessage;
use crate::models::interaction::{Interaction, InteractionData};
use crate::models::misc::Reconnect;
use crate::models::reaction_response::Reaction;
use crate::prelude::*;
use crate::utils::*;
use futures_util::FutureExt;

use thiserror::Error;

/// The result type used internally by the library.
pub type DescordResult = Result<(), DescordError>;

/// The result type used by user-defined handler functions (commands, events, components).
/// Users can use `?` with any error type in their handlers.
pub type HandlerResult = Result<(), Box<dyn std::error::Error + Send>>;

pub type HandlerFn = fn(
    Message,
    Vec<Value>,
) -> std::pin::Pin<
    Box<dyn futures_util::Future<Output = HandlerResult> + Send + 'static>,
>;

/// The error handler function type.
pub type ErrorHandlerFn = std::sync::Arc<dyn Fn(DescordError) + Send + Sync>;

/// Represents an error response from the Discord API.
#[derive(Debug, Clone)]
pub struct DiscordApiError {
    /// The HTTP status code.
    pub status: u16,
    /// The Discord-specific error code.
    pub code: Option<u64>,
    /// The error message from Discord.
    pub message: String,
}

impl std::fmt::Display for DiscordApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(code) = self.code {
            write!(f, "Discord API error {}: {} (code {})", self.status, self.message, code)
        } else {
            write!(f, "Discord API error {}: {}", self.status, self.message)
        }
    }
}

/// All possible errors that can occur within descord.
#[derive(Error, Debug)]
pub enum DescordError {
    /// An HTTP request failed (network error, timeout, etc.)
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Discord returned a non-2xx response with error details.
    #[error("{0}")]
    Api(DiscordApiError),

    /// A WebSocket error occurred.
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    /// The gateway handshake failed (critical — will panic).
    #[error("Gateway handshake failed: {0}")]
    GatewayHandshake(String),

    /// The gateway connection was closed.
    #[error("Gateway closed with code {code}: {reason}")]
    GatewayClosed {
        code: u16,
        reason: String,
    },

    /// Failed to deserialize JSON from Discord.
    #[error("JSON deserialization error: {0}")]
    DeserializeJson(#[from] nanoserde::DeJsonErr),

    /// Failed to parse JSON.
    #[error("JSON parse error: {0}")]
    JsonParse(String),

    /// A required argument was missing for a command.
    #[error("Missing required argument `{param}` for command `{command}`")]
    MissingRequiredArgument {
        command: String,
        param: String,
    },

    /// A command argument could not be parsed.
    #[error("Invalid argument `{param}`: expected {expected}, got `{got}`")]
    InvalidArgument {
        param: String,
        expected: String,
        got: String,
    },

    /// A required resource was not found.
    #[error("{resource_type} not found: {id}")]
    NotFound {
        resource_type: String,
        id: String,
    },

    /// A user's command handler returned an error.
    #[error("Command `{command}` handler error: {source}")]
    CommandHandler {
        command: String,
        #[source]
        source: Box<dyn std::error::Error + Send>,
    },

    /// A user's event handler returned an error.
    #[error("Event `{event}` handler error: {source}")]
    EventHandler {
        event: String,
        #[source]
        source: Box<dyn std::error::Error + Send>,
    },

    /// An IO error occurred.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// A catch-all for other errors.
    #[error("{0}")]
    Other(String),
}

#[macro_export]
macro_rules! implemented_enum {
    [ $vis:vis enum $name:ident { $($variant:ident),* $(,)? } ] => {
        #[derive(Debug, Clone)]
        $vis enum $name {
            $($variant($variant),)*
        }

        $(
            impl From<$variant> for $name {
                fn from(value: $variant) -> Self {
                    $name::$variant(value)
                }
            }
        )*
    };
}

/// Paramter type info (meant to be used in attribute macro).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    String,
    Int,
    Bool,
    Channel,
    User,
    Args,
}

#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Int(isize),
    Bool(bool),
    Channel(Channel),
    User(User),
    Args(Vec<String>),

    StringOption(Option<String>),
    IntOption(Option<isize>),
    BoolOption(Option<bool>),
    ChannelOption(Option<Channel>),
    UserOption(Option<User>),

    None,
}

fn parse_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current_arg = String::new();
    let mut quote_char = None;
    let mut chars = input.chars();

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' if quote_char.is_none() => {
                if !current_arg.is_empty() {
                    args.push(current_arg.clone());
                    current_arg.clear();
                }
            }
            '\'' | '"' => {
                if quote_char.is_none() {
                    quote_char = Some(c);
                    current_arg.push(c);
                } else if quote_char.unwrap() == c {
                    quote_char = None;
                    current_arg.remove(0);
                } else {
                    current_arg.push(c);
                }
            }
            _ => current_arg.push(c),
        }
    }

    if !current_arg.is_empty() {
        if quote_char.is_some() {
            args.extend(current_arg.split_whitespace().map(|s| s.to_string()));
        } else {
            args.push(current_arg);
        }
    }

    args
}
