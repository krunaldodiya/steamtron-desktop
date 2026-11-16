//! Direct IGDB Store client.
//!
//! The FastAPI backend only supplies/refreshes the Twitch application token.
//! Once that token is cached in memory, catalogue requests go straight to the
//! official IGDB host.

use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};
use tokio::sync::Mutex;

const IGDB_BASE: &str = "https://api.igdb.com/v4";
const IGDB_MAX_BYTES: usize = 1_048_576;
const MAX_PAGE: u32 = 1000;
const PAGE_SIZE: u32 = 36;
const REFRESH_SKEW_SECONDS: i64 = 120;
const REQUEST_SPACING: Duration = Duration::from_millis(260);
const GAME_FIELDS: &str = "name,summary,storyline,first_release_date,genres.name,cover.image_id,artworks.image_id,involved_companies.company.name,involved_companies.developer,involved_companies.publisher,version_title,game_type.type,platforms.name";
const DETAIL_FIELDS: &str = "name,summary,storyline,first_release_date,genres.name,cover.image_id,artworks.image_id,involved_companies.company.name,involved_companies.developer,involved_companies.publisher,version_title,game_type.type,platforms.name";

#[derive(Clone, Debug)]
struct IgdbAuth {
    client_id: String,
    access_token: String,
    expires_at: i64,
}

impl IgdbAuth {
    fn usable(&self) -> bool {
        self.expires_at > now() + REFRESH_SKEW_SECONDS
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

static AUTH_CACHE: LazyLock<Mutex<Option<IgdbAuth>>> = LazyLock::new(|| Mutex::new(None));
static LANE: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .connect_timeout(Duration::from_secs(8))
        .build()
        .expect("IGDB HTTP client")
});

fn auth_cache() -> &'static Mutex<Option<IgdbAuth>> {
    &AUTH_CACHE
}

fn lane() -> &'static Mutex<Option<Instant>> {
    &LANE
}

fn client() -> &'static reqwest::Client {
    &CLIENT
}

async fn backend_json(path: &str) -> Result<Value, String> {
    let token = steamtron_auth::access_token();
    if token.is_empty() {
        return Err("Sign in to browse the Store.".into());
    }
    let base = steamtron_auth::api_base();
    let response = client()
        .get(format!("{base}{path}"))
        .bearer_auth(token)
        .header("Accept", "application/json")
        .header("User-Agent", "steamtron-desktop")
        .send()
        .await
        .map_err(|error| format!("Could not reach SteamTron settings: {error}"))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err("The Store rejected this session. Sign in again.".into());
    }
    let text = response.text().await.map_err(|error| error.to_string())?;
    let payload: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !status.is_success() {
        let code = payload
            .get("error")
            .and_then(|value| value.get("code"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let message = payload
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("SteamTron settings returned an error.");
        if code == "IGDB_NOT_CONNECTED" {
            return Err(format!("IGDB_NOT_CONNECTED: {message}"));
        }
        if code == "IGDB_NOT_CONFIGURED" {
            return Err(format!("IGDB_NOT_CONFIGURED: {message}"));
        }
        return Err(message.to_owned());
    }
    Ok(payload)
}

async fn fetch_token() -> Result<IgdbAuth, String> {
    let payload = backend_json("/v1/igdb/token").await?;
    let client_id = payload
        .get("client_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "SteamTron settings returned no IGDB client ID.".to_string())?;
    let access_token = payload
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "SteamTron settings returned no IGDB token.".to_string())?;
    let expires_at = payload
        .get("expires_at")
        .and_then(Value::as_i64)
        .ok_or_else(|| "SteamTron settings returned no IGDB expiry.".to_string())?;
    let base_url = payload
        .get("base_url")
        .and_then(Value::as_str)
        .unwrap_or(IGDB_BASE);
    if base_url != IGDB_BASE {
        return Err("SteamTron settings returned an unexpected IGDB host.".into());
    }
    Ok(IgdbAuth {
        client_id: client_id.to_owned(),
        access_token: access_token.to_owned(),
        expires_at,
    })
}

async fn cache_token() -> Result<IgdbAuth, String> {
    let mut cache = auth_cache().lock().await;
    if let Some(auth) = cache.as_ref().filter(|auth| auth.usable()) {
        return Ok(auth.clone());
    }
    let auth = fetch_token().await?;
    *cache = Some(auth.clone());
    Ok(auth)
}

async fn clear_token() {
    *auth_cache().lock().await = None;
}

async fn wait_for_lane() {
    let mut guard = lane().lock().await;
    if let Some(previous) = *guard {
        let elapsed = previous.elapsed();
        if elapsed < REQUEST_SPACING {
            tokio::time::sleep(REQUEST_SPACING - elapsed).await;
        }
    }
    *guard = Some(Instant::now());
}

async fn query(endpoint: &str, body: &str) -> Result<Value, String> {
    if !matches!(
        endpoint,
        "games" | "genres" | "external_games" | "games/count"
    ) {
        return Err("Unsupported IGDB endpoint.".into());
    }
    if body.len() > 10_000 {
        return Err("IGDB query is too large.".into());
    }
    let mut retried = false;
    loop {
        let auth = cache_token().await?;
        wait_for_lane().await;
        let response = client()
            .post(format!("{IGDB_BASE}/{endpoint}"))
            .header("Client-ID", &auth.client_id)
            .header("Authorization", format!("Bearer {}", auth.access_token))
            .header("Accept", "application/json")
            .header("User-Agent", "steamtron-desktop")
            .body(body.to_owned())
            .send()
            .await
            .map_err(|error| format!("Could not reach IGDB: {error}"))?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            clear_token().await;
            if retried {
                return Err(
                    "IGDB_NOT_CONNECTED: IGDB rejected the token. Reconnect IGDB in Settings."
                        .into(),
                );
            }
            retried = true;
            continue;
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("Could not read the IGDB response: {error}"))?;
        if bytes.len() > IGDB_MAX_BYTES {
            return Err("IGDB returned an oversized response.".into());
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err("IGDB rate limit reached. Try again shortly.".into());
        }
        if !status.is_success() {
            return Err(format!("IGDB returned HTTP {status}."));
        }
        return serde_json::from_slice(&bytes)
            .map_err(|error| format!("IGDB returned invalid data: {error}"));
    }
}

fn text(value: Option<&Value>, fallback: &str) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

fn named(value: Option<&Value>, limit: usize) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(limit)
                .filter_map(|item| {
                    let id = item.get("id")?.as_u64()?;
                    let name = item.get("name")?.as_str()?.trim();
                    if name.is_empty() {
                        return None;
                    }
                    Some(json!({ "id": id, "name": name }))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn image_url(value: Option<&Value>, size: &str) -> Option<String> {
    let image_id = value
        .and_then(|value| value.get("image_id"))
        .and_then(Value::as_str)?;
    if image_id.is_empty()
        || !image_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }
    Some(format!(
        "https://images.igdb.com/igdb/image/upload/t_{size}/{image_id}.jpg"
    ))
}

fn company_names(game: &Value, role: &str) -> String {
    let names: Vec<String> = game
        .get("involved_companies")
        .and_then(Value::as_array)
        .map(|companies| {
            companies
                .iter()
                .filter(|company| company.get(role).and_then(Value::as_bool) == Some(true))
                .filter_map(|company| company.get("company")?.get("name")?.as_str())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    names.join(", ")
}

fn normalize(game: &Value) -> Result<Value, String> {
    let id = game
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "IGDB returned a game without an id.".to_string())?;
    let name = text(game.get("name"), "Untitled");
    let mut description = String::new();
    for field in ["summary", "storyline"] {
        if let Some(value) = game
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !description.is_empty() {
                description.push_str("\n\n");
            }
            description.push_str(value);
        }
    }
    let release_date = game
        .get("first_release_date")
        .and_then(Value::as_i64)
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let mut images = Map::new();
    if let Some(portrait) = image_url(game.get("cover"), "cover_big") {
        images.insert("portrait".into(), Value::String(portrait));
    }
    if let Some(artwork) = game
        .get("artworks")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
    {
        if let Some(hero) = image_url(Some(artwork), "screenshot_big") {
            images.insert("hero".into(), Value::String(hero));
        }
    }
    Ok(json!({
        "id": id,
        "name": name,
        "description": description,
        "release_date": release_date,
        "developers": company_names(game, "developer"),
        "publishers": company_names(game, "publisher"),
        "version_title": text(game.get("version_title"), ""),
        "genres": named(game.get("genres"), 100),
        "images": Value::Object(images),
        "steam_app_id": Value::Null,
        "game_type": game.get("game_type").and_then(|value| value.get("type")).cloned().unwrap_or(Value::Null),
        "platforms": named(game.get("platforms"), 100),
    }))
}

const PC_PLATFORMS: &str = "platforms = (6,14,3)";

fn where_clause(genre: Option<u64>) -> String {
    let mut parts = Vec::new();
    if let Some(id) = genre {
        parts.push(format!("genres = {id}"));
    }
    parts.push(PC_PLATFORMS.to_owned());
    format!("where {};", parts.join(" & "))
}

fn browse_query(search: &str, genre: Option<u64>, sort: &str, page: u32) -> String {
    let sort_clause = match sort {
        "latest" => "sort first_release_date desc;",
        "popular" => "sort total_rating_count desc;",
        "rating" => "sort total_rating desc;",
        "name_asc" => "sort name asc;",
        "name_desc" => "sort name desc;",
        _ => "sort name asc;",
    };
    let clause = if search.is_empty() {
        sort_clause.to_owned()
    } else {
        format!(
            "search {};",
            serde_json::to_string(search).unwrap_or_else(|_| "\"\"".into())
        )
    };
    let where_clause = where_clause(genre);
    format!(
        "{clause} {where_clause} fields {GAME_FIELDS}; limit {}; offset {};",
        PAGE_SIZE + 1,
        (page.saturating_sub(1)) * PAGE_SIZE
    )
}

fn count_query(genre: Option<u64>) -> String {
    where_clause(genre)
}

fn validate_query(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.chars().count() > 200 || value.chars().any(|character| character.is_control()) {
        return Err("Invalid Store search.".into());
    }
    Ok(value.to_owned())
}

fn parse_genre(value: Option<&str>) -> Result<Option<u64>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let id = value
        .parse::<u64>()
        .map_err(|_| "Invalid Store genre.".to_string())?;
    if id == 0 {
        return Err("Invalid Store genre.".into());
    }
    Ok(Some(id))
}

pub async fn status() -> Result<Value, String> {
    let cached = {
        auth_cache()
            .lock()
            .await
            .as_ref()
            .filter(|auth| auth.usable())
            .cloned()
    };
    if let Some(auth) = cached {
        return Ok(json!({
            "configured": true,
            "connected": true,
            "expires_at": auth.expires_at,
            "connect_url": format!("{}/ui/settings", steamtron_auth::api_base()),
        }));
    }
    let mut payload = backend_json("/v1/igdb/status").await?;
    if let Some(object) = payload.as_object_mut() {
        object.entry("connect_url").or_insert_with(|| {
            Value::String(format!("{}/ui/settings", steamtron_auth::api_base()))
        });
    }
    Ok(payload)
}

pub async fn genres() -> Result<Value, String> {
    let payload = query("genres", "fields name; sort name asc; limit 100;").await?;
    let items = payload
        .as_array()
        .ok_or_else(|| "IGDB returned invalid genres.".to_string())?;
    Ok(Value::Array(named(Some(&Value::Array(items.clone())), 100)))
}

pub async fn catalog(
    query_text: &str,
    genre: Option<&str>,
    sort: Option<&str>,
    page: u32,
) -> Result<Value, String> {
    let search = validate_query(query_text)?;
    let genre = parse_genre(genre)?;
    let sort = sort.unwrap_or("").trim();
    let page = page.clamp(1, MAX_PAGE);
    let games = query("games", &browse_query(&search, genre, sort, page)).await?;
    let games = games
        .as_array()
        .ok_or_else(|| "IGDB returned an invalid catalogue page.".to_string())?;
    let has_next = games.len() > PAGE_SIZE as usize && page < MAX_PAGE;
    let mut items: Vec<Value> = games
        .iter()
        .take(PAGE_SIZE as usize)
        .map(normalize)
        .collect::<Result<_, _>>()?;
    let ids: Vec<u64> = items
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_u64))
        .collect();
    if !ids.is_empty() {
        let list = ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        if let Ok(external) = query(
            "external_games",
            &format!(
                "fields game,external_game_source.name,uid,url; where game = ({list}); limit 500;"
            ),
        )
        .await
        {
            let app_ids = steam_ids(&external, &ids);
            for item in &mut items {
                let Some(game_id) = item.get("id").and_then(Value::as_u64) else {
                    continue;
                };
                if let Some(app_id) = app_ids.get(&game_id) {
                    if let Some(object) = item.as_object_mut() {
                        object.insert("steam_app_id".into(), json!(app_id));
                    }
                }
            }
        }
    }
    let mut result = json!({
        "items": items,
        "page": page,
        "has_next": has_next,
    });
    if search.is_empty() {
        if let Ok(count) = query("games/count", &count_query(genre)).await {
            if let Some(total) = count.get("count").and_then(Value::as_u64) {
                let expected = (PAGE_SIZE as u64)
                    .min(total.saturating_sub((page as u64 - 1) * PAGE_SIZE as u64))
                    as usize;
                if games.len().min(PAGE_SIZE as usize) == expected {
                    if let Some(object) = result.as_object_mut() {
                        object.insert("total_items".into(), json!(total));
                        object.insert(
                            "total_pages".into(),
                            json!(total.div_ceil(PAGE_SIZE as u64).max(1)),
                        );
                    }
                }
            }
        }
    }
    Ok(result)
}

fn steam_uid(item: &Value) -> Option<u32> {
    let source = item
        .get("external_game_source")
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)?;
    if !source.eq_ignore_ascii_case("steam") {
        return None;
    }
    let uid = item.get("uid").and_then(Value::as_str)?;
    if uid.is_empty() || !uid.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    let parsed = uid.parse::<u32>().ok()?;
    (parsed > 0).then_some(parsed)
}

fn steam_ids(external: &Value, wanted: &[u64]) -> HashMap<u64, u32> {
    let mut ids: HashMap<u64, BTreeSet<u32>> = HashMap::new();
    for item in external.as_array().into_iter().flatten() {
        let Some(game_id) = item.get("game").and_then(Value::as_u64) else {
            continue;
        };
        if !wanted.contains(&game_id) {
            continue;
        }
        let Some(uid) = steam_uid(item) else {
            continue;
        };
        ids.entry(game_id).or_default().insert(uid);
    }
    ids.into_iter()
        .filter_map(|(game_id, values)| {
            if values.len() == 1 {
                values.first().map(|app_id| (game_id, *app_id))
            } else {
                None
            }
        })
        .collect()
}

fn steam_app_id(external: &Value, game_id: u64) -> Option<u32> {
    let mut ids = BTreeSet::new();
    for item in external.as_array()? {
        if item.get("game").and_then(Value::as_u64) != Some(game_id) {
            continue;
        }
        if let Some(uid) = steam_uid(item) {
            ids.insert(uid);
        }
    }
    if ids.len() == 1 {
        ids.into_iter().next()
    } else {
        None
    }
}

pub async fn detail(game_id: u64) -> Result<Value, String> {
    if game_id == 0 {
        return Err("Invalid IGDB game id.".into());
    }
    let games = query(
        "games",
        &format!("fields {DETAIL_FIELDS}; where id = {game_id}; limit 1;"),
    )
    .await?;
    let game = games
        .as_array()
        .and_then(|items| items.first())
        .ok_or_else(|| "This title is not available in the catalog.".to_string())?;
    let mut item = normalize(game)?;
    if let Ok(external) = query(
        "external_games",
        &format!(
            "fields game,external_game_source.name,uid,url; where game = {game_id}; limit 100;"
        ),
    )
    .await
    {
        if let Some(app_id) = steam_app_id(&external, game_id) {
            if let Some(object) = item.as_object_mut() {
                object.insert("steam_app_id".into(), json!(app_id));
            }
        }
    }
    Ok(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_query_escapes_search_and_adds_genre() {
        let value = browse_query("A \"quoted\" game", Some(12), "popular", 3);
        assert!(value.contains("search \"A \\\"quoted\\\" game\";"));
        assert!(value.contains("where genres = 12 & platforms = (6,14,3);"));
        assert!(value.contains("limit 37; offset 72;"));
    }

    #[test]
    fn browse_query_rejects_invalid_page_offsets_via_page_clamp() {
        let value = browse_query("", None, "latest", MAX_PAGE);
        assert!(value.contains("offset 35964;"));
    }

    #[test]
    fn normalize_maps_images_companies_and_dates() {
        let game = json!({
            "id": 42,
            "name": "Test Game",
            "summary": "Summary",
            "storyline": "Story",
            "first_release_date": 0,
            "genres": [{"id": 5, "name": "Shooter"}],
            "cover": {"image_id": "cover_1"},
            "artworks": [{"image_id": "art_1"}],
            "involved_companies": [
                {"developer": true, "publisher": false, "company": {"name": "Dev"}},
                {"developer": false, "publisher": true, "company": {"name": "Pub"}}
            ],
            "platforms": [{"id": 6, "name": "PC (Microsoft Windows)"}]
        });
        let value = normalize(&game).expect("normalize");
        assert_eq!(value["id"], 42);
        assert_eq!(value["developers"], "Dev");
        assert_eq!(value["publishers"], "Pub");
        assert_eq!(
            value["images"]["portrait"],
            "https://images.igdb.com/igdb/image/upload/t_cover_big/cover_1.jpg"
        );
        assert_eq!(
            value["images"]["hero"],
            "https://images.igdb.com/igdb/image/upload/t_screenshot_big/art_1.jpg"
        );
        assert_eq!(value["release_date"], "1970-01-01");
    }

    #[test]
    fn steam_id_requires_one_current_game_source() {
        let value = json!([
            {"game": 42, "external_game_source": {"name": "Steam"}, "uid": "123"},
            {"game": 42, "external_game_source": {"name": "Steam"}, "uid": "123"}
        ]);
        assert_eq!(steam_app_id(&value, 42), Some(123));
        assert_eq!(steam_app_id(&value, 7), None);
    }

    #[test]
    fn batch_steam_ids_keep_only_unambiguous_matches() {
        let value = json!([
            {"game": 42, "external_game_source": {"name": "Steam"}, "uid": "123"},
            {"game": 43, "external_game_source": {"name": "GiantBomb"}, "uid": "999"},
            {"game": 44, "external_game_source": {"name": "Steam"}, "uid": "1"},
            {"game": 44, "external_game_source": {"name": "Steam"}, "uid": "2"}
        ]);
        let ids = steam_ids(&value, &[42, 43, 44]);
        assert_eq!(ids.get(&42), Some(&123));
        assert!(!ids.contains_key(&43));
        assert!(!ids.contains_key(&44));
    }

    #[test]
    fn genre_parser_rejects_non_numeric_values() {
        assert_eq!(parse_genre(Some("12")), Ok(Some(12)));
        assert_eq!(parse_genre(Some("")), Ok(None));
        assert!(parse_genre(Some("shooter")).is_err());
    }
}
