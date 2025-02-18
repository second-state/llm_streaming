use std::sync::Arc;

use bilili_rs::api::{APIClient, UserToken};

fn load_token(path: &str) -> anyhow::Result<APIClient> {
    let cookies = std::fs::read_to_string(path)?;
    let cookies: Vec<String> = cookies.lines().map(|s| s.to_string()).collect();
    let (token, jar) = UserToken::create_from_tokens(&cookies)?;
    Ok(APIClient::new(token, jar, cookies)?)
}

pub fn load_client_from_disk(path: &str) -> anyhow::Result<Arc<APIClient>> {
    let client = load_token(path)?;
    Ok(Arc::new(client))
}

pub struct BiliLiveClient {
    msg_stream: bilili_rs::live_ws::MsgStream,
}

impl BiliLiveClient {
    pub fn new(api_client: Arc<APIClient>, room_id: u64, max_retry: u32) -> Self {
        let msg_stream = bilili_rs::live_ws::connect(api_client, room_id, max_retry);
        Self { msg_stream }
    }

    #[allow(unused)]
    pub fn get_room_id(&self) -> u64 {
        self.msg_stream.room_id
    }

    pub async fn next_bili_event(&mut self) -> Option<bilili_rs::live_ws::ServerLiveMessage> {
        self.msg_stream.rx.recv().await
    }
}

impl super::StreamPlatform for BiliLiveClient {
    async fn next_event(&mut self) -> anyhow::Result<super::SteamEvent> {
        while let Some(s) = self.next_bili_event().await {
            match s {
                bilili_rs::live_ws::ServerLiveMessage::Notification(
                    bilili_rs::live_ws::NotificationMsg::DANMU_MSG { info },
                ) => {
                    return Ok(super::SteamEvent::Comment {
                        user: info.uname,
                        content: info.text,
                    });
                }
                _ => {}
            }
        }
        Err(anyhow::anyhow!("bilibili ws closed"))
    }
}

impl BiliLiveClient {
    pub fn from_config(config: crate::config::BilibiliConfig) -> anyhow::Result<Self> {
        let client = load_client_from_disk(&config.token_path)?;
        Ok(Self::new(client, config.room_id, config.max_retry))
    }
}
