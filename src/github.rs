use crate::paths::get_share_path;
use serde::Deserialize;
use serde::Serialize;
use std::fs::read_to_string;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use ureq::Error;

const CLIENT_ID: &'static str = "Ov23lihX6Mhl07qzP1Yh";

#[derive(Deserialize)]
pub struct DeviceResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u32,
    pub interval: u32,
}

pub fn request_device_code() -> Result<DeviceResponse, Error> {
    let response: DeviceResponse = dbg!(ureq::post("https://github.com/login/device/code")
        .set("Accept", "application/json")
        .send_form(&[("client_id", CLIENT_ID), ("scope", "repo")])?)
    .into_json()?;

    Ok(response)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LoginInfo {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
    pub login: String,
    pub id: u64,
    pub name: Option<String>,
    pub avatar_url: String,
    pub html_url: String,
}

impl LoginInfo {
    fn new(token: AccessTokenResponse, login: GitHubUser) -> Self {
        Self {
            access_token: token.access_token,
            token_type: token.token_type,
            scope: token.scope,
            login: login.login,
            id: login.id,
            name: login.name,
            avatar_url: login.avatar_url,
            html_url: login.html_url,
        }
    }

    fn path() -> PathBuf {
        get_share_path().join("login_info.json")
    }

    pub fn save(&self) {
        let s: String = serde_json::to_string(self).unwrap();
        let mut f = File::create(&Self::path()).unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    pub fn load() -> Option<Self> {
        let s: String = read_to_string(&Self::path()).ok()?;
        serde_json::from_str(&s).ok()
    }

    pub fn delete_login(self) {
        std::fs::remove_file(&Self::path()).unwrap();
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AccessTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
}

pub fn poll_for_token(device_code: &str, interval: u32) -> LoginInfo {
    loop {
        let res = ureq::post("https://github.com/login/oauth/access_token")
            .set("Accept", "application/json")
            .send_form(&[
                ("client_id", CLIENT_ID),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .unwrap()
            .into_json::<AccessTokenResponse>();

        match res {
            Ok(token) => {
                let user = get_user_info(&token.access_token);
                let loginfo = LoginInfo::new(token, user);
                loginfo.save();
                return loginfo;
            }
            Err(_) => {
                std::thread::sleep(std::time::Duration::from_secs(interval as u64));
            }
        }
    }
}

#[derive(Deserialize, Debug)]
struct GitHubUser {
    login: String,
    id: u64,
    name: Option<String>,
    avatar_url: String,
    html_url: String,
}

fn get_user_info(access_token: &str) -> GitHubUser {
    let response = ureq::get("https://api.github.com/user")
        .set("Authorization", &format!("Bearer {}", access_token))
        .set("User-Agent", "speki")
        .call()
        .unwrap();

    response.into_json().unwrap()
}