use std::{fs, path::PathBuf};

use directories::ProjectDirs;

mod login;
mod score;
mod solve;
mod upload;

pub use login::LoginError;
use rand::random_iter;
pub use score::{GetScoreError, ScoreStatus, TopSolutionResponseType};
pub use solve::SolveError;
pub use upload::UploadError;

#[derive(thiserror::Error, Debug)]
pub enum PbinfoUserError {
    #[error("No home directory found on system")]
    NoHomeDirError,
    #[error("Got error while reading config!\nError was: {error}")]
    ReadConfigError { error: std::io::Error },
    #[error("Got error while writing file {file}!\nError was: {error}")]
    WriteError {
        file: PathBuf,
        error: std::io::Error,
    },
    #[error("Got error while parsing config!\nError was: {error}")]
    TomlParseError { error: toml::de::Error },
}

#[derive(serde::Deserialize, Debug, serde::Serialize)]
pub struct PbinfoUser {
    email: String,
    password: String,
    ssid: String,
    form_token: String,
    user_id: String,
}

fn get_proj_dir() -> Result<ProjectDirs, PbinfoUserError> {
    Ok(
        directories::ProjectDirs::from("dev", "insertokername", "pbinfo-api")
            .ok_or_else(|| PbinfoUserError::NoHomeDirError)?,
    )
}

fn make_random_form_token() -> String {
    unsafe {
        random_iter()
            .take(40)
            .map(|i: u32| i % 16)
            .map(|i| {
                if i < 10 {
                    char::from_u32_unchecked('0' as u32 + i)
                } else {
                    char::from_u32_unchecked('a' as u32 + i - 10)
                }
            })
            .collect()
    }
}

fn make_random_form_ssid() -> String {
    unsafe {
        random_iter()
            .take(26)
            .map(|i: u32| i % 36)
            .map(|i| {
                if i < 10 {
                    char::from_u32_unchecked('0' as u32 + i)
                } else {
                    char::from_u32_unchecked('a' as u32 + i - 10)
                }
            })
            .collect()
    }
}

const CONFIG_FILE_NAME: &str = "pbinfo.toml";

impl PbinfoUser {
    pub fn new(email: String, password: String) -> Self {
        PbinfoUser {
            email: email,
            password: password,
            ssid: make_random_form_ssid(),
            form_token: make_random_form_token(),
            user_id: "".to_string(),
        }
    }

    /// Saves `config` in the ~/config dir or AppData on windows
    pub fn save_config(&self) -> Result<(), PbinfoUserError> {
        let proj_dirs = get_proj_dir()?;
        let config_dir = proj_dirs.config_dir();
        let config_file_path = config_dir.join(CONFIG_FILE_NAME);

        let parent_dir = std::path::Path::new(&config_file_path).parent().unwrap();
        if !parent_dir.exists() {
            std::fs::create_dir_all(parent_dir).map_err(|err| PbinfoUserError::WriteError {
                file: parent_dir.to_path_buf(),
                error: err,
            })?
        }

        let _ = std::fs::File::create(&config_file_path).map_err(|err| {
            PbinfoUserError::WriteError {
                file: config_file_path.to_path_buf(),
                error: err,
            }
        })?;

        std::fs::write(&config_file_path, toml::to_string(self).unwrap()).map_err(|err| {
            PbinfoUserError::WriteError {
                file: config_file_path.to_path_buf(),
                error: err,
            }
        })?;

        Ok(())
    }

    /// Gets `config` in the ~/config dir or AppData on windows
    pub fn get_config() -> Result<PbinfoUser, PbinfoUserError> {
        let proj_dirs = get_proj_dir()?;
        let config_dir = proj_dirs.config_dir();
        let config_file_path = config_dir.join(CONFIG_FILE_NAME);

        let config_file = fs::read_to_string(config_file_path)
            .map_err(|err: std::io::Error| PbinfoUserError::ReadConfigError { error: err })?;

        let parsed_conf = toml::from_str(&config_file)
            .map_err(|err: toml::de::Error| PbinfoUserError::TomlParseError { error: err })?;

        Ok(parsed_conf)
    }

    pub fn get_email(&self) -> &str {
        return self.email.as_str();
    }

    pub fn get_password(&self) -> &str {
        return self.password.as_str();
    }

    pub fn get_mut_email(&mut self) -> &mut String {
        return &mut self.email;
    }

    pub fn get_mut_password(&mut self) -> &mut String {
        return &mut self.password;
    }

    /// Makes sure a user is logged in, if not logs in the user with the
    /// provided credentials (email, password)
    pub async fn login(&mut self) -> Result<(), LoginError> {
        login::login(self).await
    }

    /// Uploads a source and returns a solution id
    pub async fn upload(&self, problem_id: &str, source: &str) -> Result<String, UploadError> {
        upload::upload(problem_id, source, self).await
    }

    /// ### !!! Under development !!!
    /// Looks up a source code solution to the given problem.
    /// If it finds it, the source code will be uploaded and a solution id
    /// will be returned
    pub async fn solve(&self, problem_id: &str) -> Result<String, SolveError> {
        solve::solve(problem_id, self).await
    }

    /// Returns information about the top solution given to a problem
    /// (if it has been solved, is the solution perfect, does problem even
    /// exist, etc...)
    pub async fn get_top_score(&self, problem_id: &str) -> TopSolutionResponseType {
        score::get_top_score(problem_id, self).await
    }

    /// Returns the score of a given solution
    pub async fn get_score(&self, sol_id: &str) -> Result<ScoreStatus, GetScoreError> {
        score::get_score(sol_id, self).await
    }

    /// Awaits the score to finish evaluation while pooling it every 1500 milliseconds
    pub async fn pool_score(&self, sol_id: &str) -> Result<serde_json::Value, GetScoreError> {
        score::pool_score(sol_id, self).await
    }
}
