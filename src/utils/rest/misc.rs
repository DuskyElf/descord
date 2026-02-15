use super::*;

pub async fn request_with_headers<T: AsRef<str>>(
    method: Method,
    endpoint: T,
    data: Option<T>,
    headers: HeaderMap<HeaderValue>,
) -> Result<Response, DescordError> {
    let mut h = get_headers();
    for (k, v) in headers.into_iter() {
        if let Some(k) = k {
            h.insert(k, v);
        }
    }

    request_int(method, endpoint, data, h).await
}

pub async fn request<T: AsRef<str>>(
    method: Method,
    endpoint: T,
    data: Option<T>,
) -> Result<Response, DescordError> {
    request_int(method, endpoint, data, get_headers()).await
}

async fn request_int<T: AsRef<str>>(
    method: Method,
    endpoint: T,
    data: Option<T>,
    headers: HeaderMap<HeaderValue>,
) -> Result<Response, DescordError> {
    let client = Client::new();
    let url = format!("{}/{}", API, endpoint.as_ref());

    let mut request_builder = client.request(method, &url);
    request_builder = request_builder.headers(headers);

    if let Some(body) = data {
        request_builder = request_builder.body(body.as_ref().to_string());
    }

    let bucket = ENDPOINT_BUCKET_MAP
        .lock()
        .await
        .get(endpoint.as_ref())
        .cloned();
    let seen;
    if let Some(bucket) = bucket {
        wait_for_rate_limit(&bucket).await;
        seen = true;
    } else {
        seen = false;
    }

    let cloned = request_builder
        .try_clone()
        .ok_or_else(|| DescordError::Other("Failed to clone request builder".to_string()))?;
    let mut response = cloned.send().await?;

    while response.status() == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);

        log::warn!(
            "Rate limited on endpoint: {}, retrying after {} seconds",
            endpoint.as_ref(),
            retry_after
        );

        sleep(Duration::from_secs_f32(retry_after)).await;

        let cloned = request_builder
            .try_clone()
            .ok_or_else(|| DescordError::Other("Failed to clone request builder".to_string()))?;
        response = cloned.send().await?;
    }

    if let Some(bucket) = response.headers().get("x-ratelimit-bucket") {
        let bucket = bucket.to_str().unwrap_or_default();
        update_rate_limit_info(response.headers(), bucket).await;
        if !seen {
            ENDPOINT_BUCKET_MAP
                .lock()
                .await
                .put(endpoint.as_ref().to_string(), bucket.to_string());
        }
    }

    // Check for API errors (4xx/5xx)
    let status = response.status();
    if status.is_client_error() || status.is_server_error() {
        let status_code = status.as_u16();
        let body = response.text().await.unwrap_or_default();

        // Try to parse Discord's error JSON: {"code": ..., "message": ...}
        let (code, message) = if let Ok(parsed) = json::parse(&body) {
            let code = parsed["code"].as_u64();
            let message = parsed["message"]
                .as_str()
                .unwrap_or(&body)
                .to_string();
            (code, message)
        } else {
            (None, body)
        };

        return Err(DescordError::Api(DiscordApiError {
            status: status_code,
            code,
            message,
        }));
    }

    Ok(response)
}

pub async fn update_rate_limit_info(headers: &HeaderMap<HeaderValue>, bucket: &str) {
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let reset = headers
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);

    let rate_limit_info = RateLimitInfo { remaining, reset };

    RATE_LIMITS
        .lock()
        .await
        .put(bucket.to_string(), rate_limit_info);
}

pub(crate) async fn wait_for_rate_limit(bucket: &str) {
    if let Some(rate_limit_info) = RATE_LIMITS.lock().await.get(bucket) {
        log::info!("Rate limit hit: {:?}", rate_limit_info);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        if rate_limit_info.remaining == 0 && rate_limit_info.reset > now {
            let delay = Duration::from_secs_f64(rate_limit_info.reset - now);
            sleep(delay).await;
        }
    }
}

pub fn get_headers() -> HeaderMap {
    let mut map = HeaderMap::new();

    map.insert("Content-Type", "application/json".parse().unwrap());
    map.insert(
        "Authorization",
        format!("Bot {}", TOKEN.lock().unwrap().as_ref().unwrap())
            .parse()
            .unwrap(),
    );

    map
}

pub async fn fetch_bot_id() -> Result<String, DescordError> {
    let response = request(Method::GET, "users/@me", None).await?;
    let text = response.text().await.map_err(DescordError::Http)?;
    let parsed = json::parse(&text)
        .map_err(|_| DescordError::JsonParse(format!("Failed to parse bot info: {}", text)))?;

    parsed["id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| DescordError::JsonParse("Missing 'id' field in bot info".to_string()))
}

/// Returns a new DM channel with a user (or return
/// an existing one). Returns a `DirectMessageChannel` object.
pub async fn fetch_dm(user_id: &str) -> Result<DirectMessageChannel, DescordError> {
    let url = format!("users/@me/channels");
    let data = json::stringify(object! {
        recipient_id: user_id
    });

    let response = request(Method::POST, &url, Some(&data)).await?;
    let text = response.text().await.map_err(DescordError::Http)?;
    DirectMessageChannel::deserialize_json(&text).map_err(DescordError::DeserializeJson)
}

pub async fn send_dm(
    user_id: &str,
    data: impl Into<CreateMessageData>,
) -> Result<Message, DescordError> {
    let dm_channel = fetch_dm(user_id).await?;
    send(&dm_channel.id, None, data).await
}
