use std::{
    collections::HashMap, convert::Infallible, error::Error, fmt::Display, io::Cursor, path::{Path, PathBuf}, sync::Arc
};

use crate::{
    config::{AdminServerConfig, Config as PFConfig, Listener},
    manager, activetracker,
};
use base64::{engine::general_purpose, Engine as _};
use lazy_static::lazy_static;
use log::info;
use regex::Regex;
use rocket::{config::TlsConfig, Request};
use rocket::{
    catch, catchers,
    config::{MutualTls, Shutdown},
    fs::NamedFile,
    get,
    http::{ContentType, Header, Status},
    post, put,
    request::FromRequest,
    response::Responder,
    routes, Response,
};
use serde::{Deserialize, Serialize};
use tokio::{fs::File, io::AsyncWriteExt, sync::RwLock};
use include_dir::{include_dir, Dir};

static STATIC: Dir<'_> = include_dir!("static");

lazy_static! {
    static ref LOCK: Arc<RwLock<bool>> = Arc::new(RwLock::new(false));
    static ref RANGE_REGEX:Regex = Regex::new(r"(?i)^bytes\s*=\s*(\d*)\s*-\s*(\d*)\s*$").unwrap();
}

struct RangeHeader {
    start: Option<usize>,
    end: Option<usize>
}

impl RangeHeader {
    pub fn has_value(&self) -> bool {
        return self.start.is_some() || self.end.is_some()
    }

    pub fn align(&self, max_size: usize) -> (usize, usize) {
        let mut start = 0;
        let mut end = max_size;
        if let Some(this_start) = self.start {
            start = this_start;
            if start > max_size {
                start = max_size;
            }
        }

        if let Some(this_end) = self.end {
            end = this_end;
            if end > max_size {
                end = max_size;
            }
            if end < start {
                end = start;
            }
        }

        return (start, end);
    }
}

#[derive(Debug)]
struct PartialContent {
    content: &'static [u8],
    content_type: ContentType,
    content_length: usize,
    range_header: Option<String>,
}

impl<'r> Responder<'r, 'static> for PartialContent {
    fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'static> {
        let mut response = Response::build();

        response
            .header(self.content_type)
            .header(Header::new("Content-Length", self.content_length.to_string()))
            .header(Header::new("Accept-Ranges", "bytes"));

        if let Some(range) = self.range_header {
            response
                .header(Header::new("Content-Range", range))
                .status(Status::PartialContent);
        } else {
            response.status(Status::Ok);
        }

        response.sized_body(self.content_length, Cursor::new(self.content)).ok()
    }
}
#[rocket::async_trait]
impl<'r> FromRequest<'r> for RangeHeader {
    type Error = Infallible;

    async fn from_request(
        request: &'r rocket::Request<'_>,
    ) -> rocket::request::Outcome<Self, Self::Error> {
        let range = request.headers().get_one("range");
        if let Some(range_str) = range {
            if let Some(caps) = RANGE_REGEX.captures(range_str) {
                let start = caps.get(1)
                                .and_then(|m| if !m.as_str().is_empty() { m.as_str().parse::<usize>().ok() } else { None });
                let end = caps.get(2)
                              .and_then(|m| if !m.as_str().is_empty() { m.as_str().parse::<usize>().ok() } else { None });
        
                // Return the parsed range
                return rocket::request::Outcome::Success(RangeHeader { start, end });
            }
        }
        return rocket::request::Outcome::Success(RangeHeader{start:None, end:None});
    }
}
#[derive(Responder, Debug, Clone)]
pub struct AuthenticationRequired {
    pub body: String,
    pub header: Header<'static>,
}

impl Default for AuthenticationRequired {
    fn default() -> Self {
        Self {
            body: "Please login".into(),
            header: Header::new(
                "WWW-authenticate",
                "Basic realm=\"Port Forwarder ACE\", charset=\"UTF-8\"",
            ),
        }
    }
}
#[catch(401)]
async fn status_401() -> AuthenticationRequired {
    Default::default()
}

#[derive(Debug)]
pub struct AuthError {}
impl Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "auth error")
    }
}

impl Error for AuthError {}
pub struct Authenticated {
    pub username: String,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Authenticated {
    type Error = AuthError;

    async fn from_request(
        request: &'r rocket::Request<'_>,
    ) -> rocket::request::Outcome<Self, Self::Error> {
        let authorization = request.headers().get_one("authorization");
        if authorization.is_none() {
            return rocket::request::Outcome::Error((Status::Unauthorized, AuthError{}));
        }
        let authorization = authorization.unwrap();
        let prefix = "basic";
        if !authorization.to_ascii_lowercase().starts_with(&prefix) {
            return rocket::request::Outcome::Error((Status::Unauthorized, AuthError{}));

        }
        let authorization = &authorization[prefix.len()..];
        let authorization = authorization.trim();
        let decoded = general_purpose::STANDARD.decode(authorization);
        if decoded.is_err() {
            return rocket::request::Outcome::Error((Status::Unauthorized, AuthError{}));
        }
        let decoded = decoded.unwrap();
        let str_result = String::from_utf8(decoded);
        if str_result.is_err() {
            return rocket::request::Outcome::Error((Status::Unauthorized, AuthError{}));
        }
        let str = str_result.unwrap();
        let idx = str.find(':');
        if idx.is_none() {
            return rocket::request::Outcome::Error((Status::Unauthorized, AuthError{}));
        }
        let idx = idx.unwrap();
        let username = &str[0..idx];
        let password = &str[idx + 1..];
        let ro = CONFIG.read().await;
        let expected_username = ro.username.clone();
        let expected_password = ro.password.clone();
        if expected_password.is_some() || expected_username.is_some() {
            let expected_username = expected_username.unwrap();
            let expected_password = expected_password.unwrap();
            if username == expected_username && password == expected_password {
                return rocket::request::Outcome::Success(Authenticated {
                    username: username.into(),
                });
            } else {
                return rocket::request::Outcome::Error((Status::Unauthorized, AuthError{}));
            }
        } else {
            return rocket::request::Outcome::Success(Authenticated {
                username: "anonymous".into(),
            });
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ISE {
    pub message: String,
}

impl ISE {
    pub fn from<T>(err: T) -> Self
    where
        T: std::fmt::Display,
    {
        Self {
            message: format!("{err}"),
        }
    }
}
impl Display for ISE {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "InternalServerError")
    }
}

impl Error for ISE {}

impl<'r> Responder<'r, 'static> for ISE {
    fn respond_to(self, _request: &'r rocket::Request<'_>) -> rocket::response::Result<'static> {
        let message = self.message.clone();
        Response::build()
            .status(Status::InternalServerError)
            .header(ContentType::Plain)
            .sized_body(message.len(), Cursor::new(message))
            .ok()
    }
}

#[get("/<file..>", rank = 9999)]
async fn static_handler(file: PathBuf, _who: Authenticated, range: RangeHeader) -> Option<PartialContent> {
    let entry = STATIC.get_file(file)?;
    let filename = entry.path().file_name()?.to_str()?;

    let content_type = ContentType::from_extension(
        filename.split('.').last().unwrap_or("")
    ).unwrap_or(ContentType::Binary);

    let content:&'static[u8] = entry.contents();
    let total_length = content.len();
    let (start, end) = range.align(total_length);
    let partial_content = &content[start..end];

    let content_range = format!("bytes {}-{}/{}", start, end, total_length);
    if range.has_value() {
        return Some(PartialContent {
            content: partial_content,
            content_type,
            content_length: partial_content.len(),
            range_header: Some(content_range),
        })
    } else {
        // Full content
        return Some(PartialContent {
            content,
            content_type,
            content_length: total_length,
            range_header: None,
        })
    }
}

#[get("/")]
#[allow(unused_variables)]
async fn index(who: Authenticated, range:RangeHeader) -> Option<PartialContent> {
    static_handler(PathBuf::from("index.html"), who, range).await
}

#[get("/apiserver/config/listeners")]
#[allow(unused_variables)]
async fn get_listener_config(who: Authenticated) -> Result<String, ISE> {
    let _ = LOCK.read().await;
    let conf: PFConfig = PFConfig::load_file(CONFIG_FILE)
        .await
        .map_err(|e| ISE::from(e))?;
    let result = serde_json::to_string(&conf.listeners).map_err(|e| ISE::from(e))?;
    return Ok(result);
}

#[get("/apiserver/config/dns")]
#[allow(unused_variables)]
async fn get_dns_config(who: Authenticated) -> Result<String, ISE> {
    let _ = LOCK.read().await;
    let conf: PFConfig = PFConfig::load_file(CONFIG_FILE)
        .await
        .map_err(|e| ISE::from(e))?;
    let result = serde_json::to_string(&conf.dns).map_err(|e| ISE::from(e))?;
    return Ok(result);
}

fn convert_error<T, X>(input: Result<T, X>) -> Result<T, ISE>
where
    X: Display,
{
    input.map_err(|e| ISE::from(e))
}

#[put("/apiserver/config/dns", data = "<data>")]
#[allow(unused_variables)]
async fn put_dns_config(who: Authenticated, data: String) -> Result<String, ISE> {
    let _ = LOCK.write().await;
    let map: HashMap<String, String> = convert_error(serde_json::from_str(&data))?;
    let mut conf: PFConfig = convert_error(PFConfig::load_file(CONFIG_FILE).await)?;
    conf.dns = map;
    let yamlout = serde_yaml::to_string(&conf).unwrap();
    let mut file_out = convert_error(File::create("config.yaml").await)?;
    let _wr = convert_error(file_out.write_all(yamlout.as_bytes()).await)?;
    Ok(data)
}

#[put("/apiserver/config/listeners", data = "<data>")]
#[allow(unused_variables)]
async fn put_listener_config(who: Authenticated, data: String) -> Result<String, ISE> {
    let _ = LOCK.write().await;
    let map: HashMap<String, Listener> = convert_error(serde_json::from_str(&data))?;
    let mut conf: PFConfig = convert_error(PFConfig::load_file(CONFIG_FILE).await)?;
    conf.listeners = map;
    let yamlout = serde_yaml::to_string(&conf).unwrap();
    let mut file_out = convert_error(File::create("config.yaml").await)?;
    convert_error(file_out.write_all(yamlout.as_bytes()).await)?;
    Ok(data)
}

#[get("/apiserver/status/listeners")]
#[allow(unused_variables)]
async fn get_listener_status(who: Authenticated) -> Result<String, ISE> {
    let result = manager::get_listener_status().await;
    let mut result_converted = HashMap::new();
    for (key, value) in result {
        let new_error = value.map_err(|x| ISE::from(x));
        result_converted.insert(key, new_error);
    }
    let result = convert_error(serde_json::to_string(&result_converted));
    return result;
}

#[get("/apiserver/stats/listeners")]
#[allow(unused_variables)]
async fn get_listener_stats(who: Authenticated) -> Result<String, ISE> {
    let stats = activetracker::get_active_list().await;
    info!("{} connections active", stats.len());
    for (id, addr) in stats.iter() {
        info!("active connection {id} from {addr}");
    }
    let result = manager::get_listener_stats().await;
    let result = convert_error(serde_json::to_string(&result));
    return result;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SimpleOperationResult {
    pub success: bool,
    pub changed: bool,
    pub message: Option<String>,
}

impl SimpleOperationResult {
    pub fn ok(message: Option<String>) -> Self {
        Self {
            success: true,
            changed: true,
            message,
        }
    }

    pub fn ok_no_change(message: Option<String>) -> Self {
        Self {
            success: true,
            changed: false,
            message,
        }
    }

    pub fn fail(message: &str) -> Self {
        Self {
            success: false,
            changed: false,
            message: Some(message.into()),
        }
    }
}
#[post("/apiserver/config/stop")]
#[allow(unused_variables)]
async fn stop(who: Authenticated) -> Result<String, ISE> {
    let _ = LOCK.write().await;
    let current_status = manager::get_run_status().await;
    let mut result = SimpleOperationResult::ok(None);
    match current_status {
        manager::Status::STARTED => {
            manager::stop().await;
        }
        manager::Status::STOPPED => {
            result = SimpleOperationResult::ok_no_change(None);
        }
        _ => {
            result = SimpleOperationResult::fail("should not happen");
        }
    }
    return Ok(serde_json::to_string(&result).unwrap());
}
#[post("/apiserver/config/start")]
async fn start(who: Authenticated) -> Result<String, ISE> {
    let _w = LOCK.write().await;
    let current_status = manager::get_run_status().await;
    match current_status {
        manager::Status::STOPPED => {
            drop(_w);
            return restart_and_apply_config(who).await;
        }
        _ => {
            let result = SimpleOperationResult::ok_no_change(None);
            return Ok(serde_json::to_string(&result).unwrap());
        }
    }
}

#[post("/apiserver/config/apply")]
#[allow(unused_variables)]
async fn restart_and_apply_config(w: Authenticated) -> Result<String, ISE> {
    let _ = LOCK.write().await;

    let conf: PFConfig = convert_error(PFConfig::load_file(CONFIG_FILE).await)?;
    {
        let mut last_w = LAST_CONFIG.write().await;
        *last_w = conf.clone();
    }
    info!("stopping manager...");
    manager::stop().await;
    info!("manager stopped");
    info!("starting manager...");
    let result = convert_error(manager::start(conf).await)?;
    info!("manager started");
    let mut result_converted = HashMap::new();
    for (key, value) in result {
        let new_error = value.map_err(|x| ISE::from(x));
        result_converted.insert(key, new_error);
    }
    let result = convert_error(serde_json::to_string(&result_converted));
    return result;
}

#[post("/apiserver/config/reset")]
#[allow(unused_variables)]
async fn reset_original_config(who: Authenticated) -> Result<String, ISE> {
    let _ = LOCK.write().await;
    let old = LAST_CONFIG.read().await;
    let old_dns = old.dns.clone();
    let old_listeners = old.listeners.clone();

    let mut conf: PFConfig = convert_error(PFConfig::load_file(CONFIG_FILE).await)?;
    conf.listeners = old_listeners;
    conf.dns = old_dns;
    let yamlout = serde_yaml::to_string(&conf).unwrap();
    let mut file_out = convert_error(File::create("config.yaml").await)?;
    let _wr = convert_error(file_out.write_all(yamlout.as_bytes()).await)?;
    let json_result = serde_json::to_string("OK").unwrap();
    Ok(json_result)
}

static CONFIG_FILE: &str = "config.yaml";

lazy_static! {
    static ref CONFIG: Arc<RwLock<AdminServerConfig>> = Arc::new(RwLock::new(Default::default()));
    static ref LAST_CONFIG: Arc<RwLock<PFConfig>> = Arc::new(RwLock::new(Default::default()));
}


pub async fn init(config: &PFConfig) {
    info!("initializing adminserver...");
    {
        let mut w = LAST_CONFIG.write().await;
        *w = config.clone();
    }
    let admin_config = (&config.admin_server).clone();
    match admin_config {
        Some(what) => {
            let mut w = CONFIG.write().await;
            *w = what;
        }
        None => {
            return;
        }
    }
    info!("initialized admin server");
}

fn choose<T>(first: &Option<T>, default: T) -> T
where
    T: Clone,
{
    let first_c = first.clone();
    match first_c {
        Some(what) => {
            return what;
        }
        _ => {
            return default;
        }
    }
}
pub async fn run_rocket() -> Result<(), Box<dyn Error>> {
    let config = CONFIG.read().await;
    let mut figment = rocket::Config::figment()
        .merge(("port", choose(&config.bind_port, 48888)))
        .merge(("address", choose(&config.bind_address, "0.0.0.0".into())))
        .merge((
            "log_level",
            choose(&config.rocket_log_level, "normal".into()),
        ));

    let enable_tls = choose(&config.tls, false);
    if enable_tls {
        let server_pem = choose(&config.tls_cert, "server.pem".into());
        let server_key = choose(&config.tls_key, "server.key".into());
        let mut tls_config = TlsConfig::from_paths(server_pem, server_key);
        let enable_mtls = choose(&config.mutual_tls, false);
        if enable_mtls {
            let ca_cert = choose(&config.tls_ca_cert, "ca.pem".into());
            tls_config = tls_config.with_mutual(MutualTls::from_path(ca_cert).mandatory(true));
        }
        figment = figment.merge(("tls", tls_config));
    }
    let shutdown: Shutdown = Default::default();
    figment = figment.merge(("shutdown", shutdown));

    let _r = rocket::custom(figment)
        .register("/", catchers![status_401])
        .mount(
            "/",
            routes![
                index,
                get_listener_config,
                get_dns_config,
                put_dns_config,
                put_listener_config,
                reset_original_config,
                restart_and_apply_config,
                start,
                stop,
                get_listener_stats,
                get_listener_status,
                static_handler,
            ],
        )
        .launch()
        .await?;
    info!("Rocket over");
    return Ok(());
}
