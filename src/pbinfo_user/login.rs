use std::collections::HashMap;

use reqwest::{header::InvalidHeaderValue, Response};
use thiserror::Error;

use crate::pbinfo_user::PbinfoUser;

#[derive(Error, Debug)]
pub enum LoginError {
    #[error("Error: didn't get back an ssid cookie!\nLogin failed!\nUsername/Email and password may be incorect! OR Maybe you tried logging in too many times!")]
    NoCookieError,
    #[error("Error: Couldn't parse a header!\nGot error:\n{err}")]
    HeaderParseError { err: String },
    #[error("Error: Couldn't parse the following cookie:\n{cookie}!\nGot error:\n{err}")]
    CookieParseError { cookie: String, err: String },
    #[error("Error: Couldn't send a request to the url: {url}\nGot error:\n{err}")]
    RequestSendError { url: String, err: String },
    #[error("Error: Couldn't build a reqwest client\nGot error:\n{err}")]
    RequestBuildError { err: String },
    #[error("Error: Couldn't parse a response\nGot error:\n{err}")]
    ResponseParseError { err: String },
    #[error("Error: Couldn't parse the following text to a json:\n{json}\nGot error:\n{err}")]
    JsonParseError { json: String, err: String },
    #[error("Error: Utilizator / parola incorecte!")]
    IncorrectUsernameOrPasswordError,
    #[error("Error: There was no user id found in the body of pbinfo!")]
    NoUserIdError,
}

impl From<InvalidHeaderValue> for LoginError {
    fn from(err: InvalidHeaderValue) -> Self {
        Self::HeaderParseError {
            err: err.to_string(),
        }
    }
}

fn try_get_ssid(response: &reqwest::Response) -> Result<String, LoginError> {
    let new_ssid_header = response
        .headers()
        .get("set-cookie")
        .ok_or_else(|| LoginError::NoCookieError)?
        .to_str()
        .map_err(|err| LoginError::HeaderParseError {
            err: format!(
                "Couldn't make a string out of the HeaderValue, got error: {}",
                err.to_string()
            ),
        })?;

    let new_ssid_cookie =
        new_ssid_header
            .split(";")
            .next()
            .ok_or_else(|| LoginError::HeaderParseError {
                err: format!(
                    "Couldn't find anything after the first ';' in the header:\n{new_ssid_header}"
                ),
            })?;

    new_ssid_cookie
        .split("=")
        .nth(1)
        .ok_or_else(|| LoginError::CookieParseError {
            cookie: new_ssid_cookie.to_string(),
            err: "Couldn't find anything after the '=' sign!".to_string(),
        })
        .map(|x| x.to_string())
}

async fn get_login_response(pbinfo_user: &mut PbinfoUser) -> Result<Response, LoginError> {
    let client: reqwest::Client =
        reqwest::Client::builder()
            .build()
            .map_err(|err| LoginError::RequestBuildError {
                err: err.to_string(),
            })?;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Origin", "https://www.pbinfo.ro".parse()?);
    headers.insert("Referer", "https://www.pbinfo.ro/".parse()?);
    headers.insert("Cookie", format!("SSID={}", pbinfo_user.ssid).parse()?);

    // 'Content-Type: application/x-www-form-urlencoded; charset=UTF-8'

    let mut form_data = HashMap::new();
    form_data.insert("user", pbinfo_user.email.as_str());
    form_data.insert("parola", pbinfo_user.password.as_str());
    form_data.insert("form_token", pbinfo_user.form_token.as_str());

    let login_url = "https://www.pbinfo.ro/ajx-module/php-login.php";
    let response = client
        .request(reqwest::Method::POST, login_url)
        .headers(headers)
        .form(&form_data)
        .send()
        .await
        .map_err(|err| LoginError::RequestSendError {
            url: login_url.to_string(),
            err: err.to_string(),
        })?;
    Ok(response)
}

async fn get_login_response_body(
    response: reqwest::Response,
) -> Result<serde_json::Value, LoginError> {
    let text = response
        .text()
        .await
        .map_err(|err| LoginError::ResponseParseError {
            err: err.to_string(),
        })?;

    let table: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| LoginError::JsonParseError {
            json: text,
            err: err.to_string(),
        })?;

    Ok(table)
}

/// Returns the user id for a user. This must be scraped out of the
/// source html with a bit of rust magic
async fn get_user_id(pbinfo_user: &mut PbinfoUser) -> Result<String, LoginError> {
    let client: reqwest::Client =
        reqwest::Client::builder()
            .build()
            .map_err(|err| LoginError::RequestBuildError {
                err: err.to_string(),
            })?;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Origin", "https://www.pbinfo.ro".parse()?);
    headers.insert("Referer", "https://www.pbinfo.ro/".parse()?);
    headers.insert("Cookie", format!("SSID={}", pbinfo_user.ssid).parse()?);

    let url = "https://www.pbinfo.ro".to_string();

    let response = client
        .request(reqwest::Method::GET, url.as_str())
        .headers(headers)
        .send()
        .await
        .map_err(|e| LoginError::RequestSendError {
            url: url,
            err: e.to_string(),
        })?;

    let body = response
        .text()
        .await
        .map_err(|err| LoginError::ResponseParseError {
            err: err.to_string(),
        })?;

    // we are looking for the user id in a string that looks something
    // like this:
    // {page html}
    // user_autentificat = {"id":XXXXXX,
    // {continuation page html}
    let marker = "user_autentificat = {\"id\":";
    let before =
        body.split(marker)
            .skip(1)
            .next()
            .ok_or_else(|| LoginError::ResponseParseError {
                err: "Didn't find anything after user_autentificat = {\"id\":".to_string(),
            })?;

    let user_id: String = before.chars().take_while(|&c| c != ',').collect();

    Ok(user_id)
}

/// Makes sure a user is logged in, if not logs in the user with the
/// provided credentials
pub async fn login(pbinfo_user: &mut PbinfoUser) -> Result<(), LoginError> {
    let user_id = get_user_id(pbinfo_user).await?;
    if user_id != "0" && user_id != "" {
        return Ok(());
    }
    pbinfo_user.user_id = user_id;

    let response = get_login_response(pbinfo_user).await?;
    let maybe_ssid = try_get_ssid(&response);

    let val = get_login_response_body(response).await?;
    if val["raspuns"] == "Formularul a expirat. Încearcă din nou!" {
        pbinfo_user.form_token = val["form_token"]
            .to_string()
            .trim_start_matches("\"")
            .trim_end_matches("\"")
            .to_string();
    } else {
        pbinfo_user.ssid = maybe_ssid?;
        pbinfo_user.user_id = get_user_id(pbinfo_user).await?;
        return Ok(());
    }

    let response = get_login_response(pbinfo_user).await?;
    let maybe_ssid = try_get_ssid(&response);
    let val = get_login_response_body(response).await?;
    if val["raspuns"] == "Utilizator/parola incorecte!" {
        return Err(LoginError::IncorrectUsernameOrPasswordError);
    }
    pbinfo_user.ssid = maybe_ssid?;
    pbinfo_user.user_id = get_user_id(pbinfo_user).await?;
    Ok(())
}
