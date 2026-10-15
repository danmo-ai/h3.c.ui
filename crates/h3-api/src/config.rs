use std::path::PathBuf;

use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Parser)]
#[command(name = "h3-api", about = "H3 Metal REST API")]
pub struct Cli {
    #[arg(long, env = "H3_API_HOST", default_value = "127.0.0.1")]
    pub host: String,

    #[arg(long, env = "H3_API_PORT", default_value_t = 7810)]
    pub port: u16,

    /// Control-plane directory (config, logs, default model/output paths).
    #[arg(long, env = "H3_UI_HOME")]
    pub home: Option<PathBuf>,

    /// Optional directory of built frontend assets (served at `/`).
    #[arg(long, env = "H3_UI_STATIC")]
    pub static_dir: Option<PathBuf>,
}

/// Where to fetch MiniMax-H3 weights from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DownloadSource {
    #[default]
    Huggingface,
    HfMirror,
    Modelscope,
}

impl DownloadSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Huggingface => "huggingface",
            Self::HfMirror => "hf_mirror",
            Self::Modelscope => "modelscope",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "huggingface" | "hf" => Some(Self::Huggingface),
            "hf_mirror" | "mirror" => Some(Self::HfMirror),
            "modelscope" | "ms" => Some(Self::Modelscope),
            _ => None,
        }
    }

    pub fn default_repo(self) -> &'static str {
        match self {
            Self::Huggingface | Self::HfMirror => "MiniMaxAI/MiniMax-H3",
            Self::Modelscope => "MiniMax/MiniMax-H3",
        }
    }

    /// Hugging Face hub endpoint override (`None` for ModelScope).
    pub fn hf_endpoint(self) -> Option<&'static str> {
        match self {
            Self::Huggingface => Some("https://huggingface.co"),
            Self::HfMirror => Some("https://hf-mirror.com"),
            Self::Modelscope => None,
        }
    }

    pub fn all() -> &'static [DownloadSource] {
        &[Self::Huggingface, Self::HfMirror, Self::Modelscope]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub model_dir: PathBuf,
    pub output_dir: PathBuf,
    pub cache_enabled: bool,
    pub hf_repo: String,
    #[serde(default = "default_modelscope_repo")]
    pub modelscope_repo: String,
    /// Separate Turbo LoRA hub (not part of the base MiniMax-H3 tree).
    #[serde(default = "default_turbo_lora_hf_repo")]
    pub turbo_lora_hf_repo: String,
    #[serde(default = "default_turbo_lora_modelscope_repo")]
    pub turbo_lora_modelscope_repo: String,
    #[serde(default)]
    pub download_source: DownloadSource,
}

fn default_modelscope_repo() -> String {
    "MiniMax/MiniMax-H3".to_string()
}

fn default_turbo_lora_hf_repo() -> String {
    "larryvrh/MiniMax-H3-Turbo-Lora".to_string()
}

fn default_turbo_lora_modelscope_repo() -> String {
    "larryvrh/MiniMax-H3-Turbo-Lora".to_string()
}

impl AppConfig {
    pub fn load_or_default(home: &PathBuf) -> Self {
        let path = home.join("config.json");
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(cfg) = serde_json::from_slice::<AppConfig>(&bytes) {
                return cfg;
            }
        }
        let cfg = Self {
            model_dir: home.join("models").join("MiniMax-H3"),
            output_dir: home.join("outputs"),
            cache_enabled: true,
            hf_repo: "MiniMaxAI/MiniMax-H3".to_string(),
            modelscope_repo: default_modelscope_repo(),
            turbo_lora_hf_repo: default_turbo_lora_hf_repo(),
            turbo_lora_modelscope_repo: default_turbo_lora_modelscope_repo(),
            download_source: DownloadSource::default(),
        };
        let _ = cfg.save(home);
        cfg
    }

    pub fn repo_for_source(&self, source: DownloadSource) -> &str {
        match source {
            DownloadSource::Huggingface | DownloadSource::HfMirror => &self.hf_repo,
            DownloadSource::Modelscope => &self.modelscope_repo,
        }
    }

    pub fn turbo_lora_repo_for_source(&self, source: DownloadSource) -> &str {
        match source {
            DownloadSource::Huggingface | DownloadSource::HfMirror => &self.turbo_lora_hf_repo,
            DownloadSource::Modelscope => &self.turbo_lora_modelscope_repo,
        }
    }

    pub fn save(&self, home: &PathBuf) -> anyhow::Result<()> {
        std::fs::create_dir_all(home)?;
        std::fs::create_dir_all(&self.model_dir)?;
        std::fs::create_dir_all(&self.output_dir)?;
        let path = home.join("config.json");
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

pub fn default_home() -> PathBuf {
    dirs_home()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".h3-ui")
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
