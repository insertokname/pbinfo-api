use std::{fs, path::PathBuf};

use directories::ProjectDirs;

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
    pub email: String,
    pub password: String,
    pub ssid: String,
    pub form_token: String,
    pub user_id: String,
}

fn get_proj_dir() -> Result<ProjectDirs, PbinfoUserError> {
    Ok(
        directories::ProjectDirs::from("dev", "insertokername", "pbinfo-api")
            .ok_or_else(|| PbinfoUserError::NoHomeDirError)?,
    )
}

const CONFIG_FILE_NAME: &str = "pbinfo.toml";

impl PbinfoUser {
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
}
