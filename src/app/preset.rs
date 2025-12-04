use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Preset {
    pub inner: UnprocessedPreset,
    pub assignments: Vec<usize>,
    #[serde(default)]
    pub target_img: Option<Vec<u8>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UnprocessedPreset {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub source_img: Vec<u8>,
}
