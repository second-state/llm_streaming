use std::collections::LinkedList;

use crate::llm::llm::Content;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LLMConfig {
    pub llm_chat_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub sys_prompts: Vec<Content>,
    #[serde(default)]
    pub dynamic_prompts: LinkedList<Content>,
    pub history: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FishTTS {
    pub api_key: String,
    pub speaker: String,
    pub vtb_name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StableTTS {
    pub base_url: String,
    pub speaker: String,
    pub vtb_name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "platform")]
pub enum TTSConfig {
    Stable(StableTTS),
    Fish(FishTTS),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "platform")]
pub enum StreamPlatFormConfig {
    Bilibili(BilibiliConfig),
    Restream(RestreamConfig),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BilibiliConfig {
    pub room_id: u64,
    pub max_retry: u32,
    pub token_path: String,
    pub max_comment: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RestreamConfig {
    pub url: String,
    pub max_comment: usize,
}

#[test]
fn test_serde() {
    let bilibili = StreamPlatFormConfig::Bilibili(BilibiliConfig {
        room_id: 123456,
        max_retry: 3,
        token_path: "token.txt".to_string(),
        max_comment: 20,
    });
    let s = serde_json::to_string(&bilibili).unwrap();
    println!("{}", s);

    let restream = StreamPlatFormConfig::Restream(RestreamConfig {
        url: "ws://httpbin.org".to_string(),
        max_comment: 20,
    });
    let s = serde_json::to_string(&restream).unwrap();
    println!("{}", s);

    let config = Config {
        listen: "0.0.0.0:8080".to_string(),
        llm: LLMConfig {
            llm_chat_url: "http://llm.com".to_string(),
            sys_prompts: vec![Content {
                role: crate::llm::llm::Role::System,
                message: "system".to_string(),
            }],
            dynamic_prompts: LinkedList::new(),
            history: 10,
            api_key: None,
        },
        platform: bilibili,
        tts: TTSConfig::Stable(StableTTS {
            base_url: "http://tts.com".to_string(),
            speaker: "speaker".to_string(),
            vtb_name: "vtb".to_string(),
        }),
        downstream: DownstreamConfig {
            update_title_url: "http://update.com".to_string(),
            segment_url: "http://segment.com".to_string(),
        },
    };
    let config_str = serde_json::to_string(&config).unwrap();
    println!("{}", config_str);
    let config: Config = serde_json::from_str(&config_str).unwrap();
    println!("{:?}", config);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DownstreamConfig {
    pub update_title_url: String,
    pub segment_url: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub listen: String,
    pub llm: LLMConfig,
    pub tts: TTSConfig,
    pub platform: StreamPlatFormConfig,
    pub downstream: DownstreamConfig,
}
