use std::sync::LazyLock;

use reqwest::StatusCode;
use serde_json::Value;

use super::upload::upload;
use crate::pbinfo_user::PbinfoUser;

use super::UploadError;

#[derive(thiserror::Error, Debug)]
enum GetSolutionError {
    #[error("Couldn't find a solution for the problem {problem_id} on github codulluiandrei")]
    NoGithubSolution { problem_id: String },
    #[error("Couldn't create a reqwest client!\nGot error {err}")]
    CreateReqwestClientError { err: String },
    #[error("Couldn't send a request to the url: '{url}'\nGot error {err}")]
    SendRequestError { err: String, url: String },
    #[error("Couldn't parse the text in a response from url: '{url}'\nGot error {err}")]
    RequestParseTextError { err: String, url: String },
}

static SOLUTIONS: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("solutions.json")).unwrap());

async fn get_raw_solution(
    problem_id: &str,
    costume_solutions: Option<&Value>,
) -> Result<String, GetSolutionError> {
    if let Some(some) = costume_solutions {
        if some[problem_id].is_string() {
            println!("Found a solution user provided custom_solutions!");
            return Ok(some[problem_id].to_string());
        }
    }

    if SOLUTIONS[problem_id].is_string() {
        println!("Found a solution in builtin solutions!");
        return Ok(SOLUTIONS[problem_id].to_string());
    }

    let client = reqwest::Client::builder().build().map_err(|err| {
        GetSolutionError::CreateReqwestClientError {
            err: err.to_string(),
        }
    })?;

    let url = format!("https://raw.githubusercontent.com/codulluiandrei/pbinfo/refs/heads/main/pbinfo-{problem_id}/main.cpp");
    let response = client
        .request(reqwest::Method::GET, &url)
        .send()
        .await
        .map_err(|err| GetSolutionError::SendRequestError {
            err: err.to_string(),
            url: url.clone(),
        })?;

    if response.status() != StatusCode::OK {
        return Err(GetSolutionError::NoGithubSolution {
            problem_id: problem_id.to_string(),
        });
    }

    let text = response
        .text()
        .await
        .map_err(|err| GetSolutionError::RequestParseTextError {
            err: err.to_string(),
            url,
        })?;
    Ok(text)
}

#[derive(thiserror::Error, Debug)]
pub enum SolveError {
    #[error("Couldn't get a solution for the problem {problem_id}\nGot error{err}")]
    GetSolutionError { problem_id: String, err: String },
    #[error("Couldn't upload a solution for the problem {problem_id}\nGot error{}",err.to_string())]
    UploadError {
        problem_id: String,
        err: UploadError,
    },
}

async fn solve_helper(
    problem_id: &str,
    pbinfo_user: &PbinfoUser,
    costume_solutions: Option<&Value>,
) -> Result<String, SolveError> {
    let correct_solution = get_raw_solution(problem_id, costume_solutions)
        .await
        .map_err(|err| SolveError::GetSolutionError {
            problem_id: problem_id.to_string(),
            err: err.to_string(),
        })?;

    upload(&problem_id, &correct_solution, pbinfo_user)
        .await
        .map_err(|err| SolveError::UploadError {
            problem_id: problem_id.to_string(),
            err: err,
        })
}

pub async fn solve(problem_id: &str, pbinfo_user: &PbinfoUser) -> Result<String, SolveError> {
    solve_helper(problem_id, pbinfo_user, None).await
}

pub async fn costume_solve(
    problem_id: &str,
    costume_solutions: &Value,
    pbinfo_user: &PbinfoUser,
) -> Result<String, SolveError> {
    solve_helper(problem_id, pbinfo_user, Some(costume_solutions)).await
}
