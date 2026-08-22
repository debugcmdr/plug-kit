use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "version")]
    pub version: String,
    pub author: Option<String>,
    pub homepage: Option<String>,
    pub icon: Option<String>,
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(default, rename = "pluginDependencies")]
    pub plugin_dependencies: Option<std::collections::HashMap<String, String>>,
    #[serde(rename = "bridgeVersion")]
    pub bridge_version: String,
    #[serde(default, rename = "minAppVersion")]
    pub min_app_version: Option<String>,
    #[serde(default, rename = "expectedSha256")]
    pub expected_sha256: Option<String>,
    // binaryUrl 属于市场清单(ManifestSummary),运行时 manifest 不含此项
    #[serde(default, rename = "binaryUrl")]
    pub binary_url: Option<String>,
    pub entry: ManifestEntry,
    pub commands: std::collections::HashMap<String, CommandDefinition>,
    #[serde(default)]
    pub ui: Option<UiDefinition>,
    #[serde(rename = "releaseUrl")]
    pub release_url: Option<String>,
}

impl Manifest {
    /// Whether the plugin ships an iframe UI.
    pub fn has_ui(&self) -> bool {
        self.ui.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub executable: String,
    #[serde(default = "default_tool_dir")]
    pub tool_dir: String,
}

fn default_tool_dir() -> String {
    "tool".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandDefinition {
    pub stdin_args: Vec<String>,
    #[serde(rename = "output")]
    pub output_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiDefinition {
    #[serde(rename = "type")]
    pub ui_type: String,
    pub html: String,
}

impl Manifest {
    pub fn tool_dir(&self) -> &str {
        &self.entry.tool_dir
    }
}
