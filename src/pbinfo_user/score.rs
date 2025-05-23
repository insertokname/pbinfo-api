use std::{future::Future, time::Duration};

use reqwest::header::{HeaderValue, InvalidHeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::pbinfo_user::PbinfoUser;

#[derive(Error, Debug)]
pub enum GetScoreError {
    #[error("There was an error while getting the status of a score!\nError was {}",(*err).to_string())]
    GenericError { err: Box<dyn std::error::Error> },
    #[error(
        "Error: Couldn't parse a response json while getting a score:\n{json}\nError was: {err}"
    )]
    ParseJsonError { json: String, err: String },
    #[error("Error: The execution of a problem timed out!\nA problem took longer than 30 seconds to evaluate!")]
    TimeoutError,
}

impl From<reqwest::Error> for GetScoreError {
    fn from(err: reqwest::Error) -> Self {
        GetScoreError::GenericError { err: Box::new(err) }
    }
}

impl From<InvalidHeaderValue> for GetScoreError {
    fn from(err: InvalidHeaderValue) -> Self {
        GetScoreError::GenericError { err: Box::new(err) }
    }
}

pub enum ScoreStatus {
    DoneExecuting { value: Value },
    StillExecuting,
}

/// Returns the score of a given solution
pub async fn get_score(
    sol_id: &str,
    pbinfo_user: &PbinfoUser,
) -> Result<ScoreStatus, GetScoreError> {
    let client = reqwest::Client::builder().build()?;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Cookie",
        HeaderValue::from_str(&format!("SSID={}", pbinfo_user.ssid))?,
    );
    let request = client
        .request(
            reqwest::Method::POST,
            format!(
                "https://www.pbinfo.ro/ajx-module/ajx-solutie-detalii-evaluare.php?force_reload&id={sol_id}"
            ),
        )
        .headers(headers);

    let response = request.send().await?;
    let text = response.text().await?;

    let table: Value =
        serde_json::from_str(&text).map_err(|err| GetScoreError::ParseJsonError {
            json: text,
            err: err.to_string(),
        })?;

    if table["status_sursa"] == "executing" || table["status_sursa"] == "pending" {
        return Ok(ScoreStatus::StillExecuting);
    }

    Ok(ScoreStatus::DoneExecuting { value: table })
}

/// Awaits the score to finish evaluation while pooling it every 1500 milliseconds
pub async fn pool_score(
    solution_id: &str,
    pbinfo_user: &PbinfoUser,
) -> Result<Value, GetScoreError> {
    let mut tries = 60;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    while tries > 0 {
        match get_score(solution_id, pbinfo_user).await? {
            ScoreStatus::StillExecuting => {
                tokio::time::sleep(Duration::from_millis(1500)).await;
            }
            ScoreStatus::DoneExecuting { value } => {
                // one last force_reload of the score so that pbinfo
                // actually displays the score on the site
                let _ = get_score(solution_id, pbinfo_user).await;
                return Ok(value);
            }
        }
        tries -= 1;
    }

    Err(GetScoreError::TimeoutError)
}

async fn check_problem_exists(
    problem_id: &str,
    pbinfo_user: &PbinfoUser,
) -> Result<bool, Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder().build()?;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Cookie", format!("SSID={}", pbinfo_user.ssid).parse()?);
    let request = client
        .request(
            reqwest::Method::POST,
            format!("https://www.pbinfo.ro/probleme/{problem_id}"),
        )
        .headers(headers);

    let response = request.send().await?;
    return Ok(response.status() == reqwest::StatusCode::OK);
}

async fn try_repeated<T, E, F, Fut>(attempts: u32, f: F) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    for _ in 0..attempts {
        match f().await {
            Ok(ok) => return Ok(ok),
            Err(_) => continue,
        }
    }
    f().await
}

/// Information about a certain problem like: if the user solved it,
/// if they do, does their solution have 100 points, etc..
#[derive(Debug, Serialize, Deserialize)]
pub enum TopSolutionResponseType {
    /// This problem has been solved and has 100 points
    PerfectSolution,
    /// This problem has only been attempted and doesn't have 100 points
    ImperfectSolution,
    /// This problem hasn't been atempted
    NoSolution,
    /// This problem was not found
    ProblemNotFound,
    /// There was an error while parsing the response of the server
    PageError(String),
}

async fn get_last_n_solutions(
    problem_id: &str,
    sol_number: u32,
    pbinfo_user: &PbinfoUser,
) -> Result<Value, Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder().build()?;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Cookie", format!("SSID={}", pbinfo_user.ssid).parse()?);
    let request = client
        .request(
            reqwest::Method::POST,
            format!(
                "https://www.pbinfo.ro/ajx-module/ajx-solutii-lista-json.php?id_problema={problem_id}&id_user={}&numar_solutii={sol_number}"
                , pbinfo_user.user_id
            ),
        )
        .headers(headers);

    let response = request.send().await?;
    let text = response.text().await?;
    Ok(serde_json::from_str(&text)?)
}

/// Returns information about the top solution given to a problem
/// (if it has been solved, is the solution perfect, does problem even
/// exist, etc...)
pub async fn get_top_score(problem_id: &str, pbinfo_user: &PbinfoUser) -> TopSolutionResponseType {
    match try_repeated(3, || check_problem_exists(problem_id, pbinfo_user)).await {
        Ok(false) => return TopSolutionResponseType::ProblemNotFound,
        Ok(true) => (),
        Err(err) => return TopSolutionResponseType::PageError(err.to_string()),
    };

    let last_solution =
        match try_repeated(3, || get_last_n_solutions(problem_id, 1, pbinfo_user)).await {
            Ok(ok) => ok,
            Err(err) => return TopSolutionResponseType::PageError(err.to_string()),
        };

    let sol_number = match last_solution["numar_total_solutii"].as_i64(){
        Some(some) if u32::try_from(some).is_ok() => some as u32,
        Some(some) => return TopSolutionResponseType::PageError(format!("numar_total_solutii couldn't be parsed to an u32\nnumar_total_solutii was {some}")),
        None=> return TopSolutionResponseType::PageError(format!("numar_total_solutii couldn't be found in response json as an int!\nResponse json was: {}",last_solution.to_string())),
    };

    if sol_number == 0 {
        return TopSolutionResponseType::NoSolution;
    }

    let all_solutions = match try_repeated(3, || {
        get_last_n_solutions(problem_id, sol_number as u32, pbinfo_user)
    })
    .await
    {
        Ok(ok) if ok["surse"].is_array() => {
            let array = ok["surse"].as_array().unwrap().clone();
            array
        }
        Ok(_) => {
            return TopSolutionResponseType::PageError(format!(
                "surse was not an array\nResponse was: {}",
                last_solution.to_string()
            ))
        }
        Err(err) => return TopSolutionResponseType::PageError(err.to_string()),
    };

    let scores = match all_solutions
        .iter()
        .map(|sol| sol["scor"].as_str().unwrap_or("-2").parse::<i64>())
        .collect::<Result<Vec<i64>, _>>()
    {
        Ok(ok) => ok,
        Err(err) => {
            return TopSolutionResponseType::PageError(format!(
            "Couldn't parse a score for a solution!\nSolution list was{:?}\nParse Error was {err}",
            all_solutions
        ))
        }
    };

    if scores.iter().any(|score| *score == 100) {
        return TopSolutionResponseType::PerfectSolution;
    } else {
        return TopSolutionResponseType::ImperfectSolution;
    }
}
