mod channel;
mod message;
mod role;
mod user;

use super::*;

pub use channel::*;
pub use message::*;
pub use role::*;
pub use user::*;

/* Misc functions */

pub async fn fetch_application_commands(bot_id: &str) -> Result<Vec<ApplicationCommand>, DescordError> {
    let resp = request(
        Method::GET,
        format!("applications/{}/commands", bot_id).as_str(),
        None,
    )
    .await?
    .text()
    .await
    .map_err(DescordError::Http)?;

    DeJson::deserialize_json(&resp).map_err(|e| {
        log::error!("Failed to deserialize application commands: {}", e);
        DescordError::DeserializeJson(e)
    })
}

pub async fn fetch_guild(guild_id: &str) -> Result<Guild, DescordError> {
    if let Some(guild) = GUILD_CACHE.lock().await.get(guild_id).cloned() {
        return Ok(guild);
    }

    let url = format!("guilds/{guild_id}");
    let resp = request(Method::GET, &url, None).await?.text().await.map_err(DescordError::Http)?;
    let guild = Guild::deserialize_json(&resp).map_err(DescordError::DeserializeJson)?;

    GUILD_CACHE
        .lock()
        .await
        .put(guild_id.to_string(), guild.clone());
    Ok(guild)
}
