use std::{collections::LinkedList, sync::Arc};

use axum::{
    extract::Multipart,
    routing::{any, post},
    Extension, Router,
};
use bytes::Bytes;
use reqwest::{multipart::Part, StatusCode};

use crate::{
    config::{DownstreamConfig, FishTTS, LLMConfig, StableTTS, TTSConfig},
    llm::{fish_tts, llm::Content, llm_stable, tts},
    stream_platform::CommentTx,
};

pub fn router(
    llm_config: LLMConfig,
    tts_config: TTSConfig,
    downstream_config: DownstreamConfig,
    stream_tx: tokio::sync::mpsc::UnboundedSender<CommentTx>,
) -> Router {
    let (store_tx, store_rx) = tokio::sync::mpsc::unbounded_channel();
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    let store = PodcastStore::new(store_tx);
    tokio::spawn(store.run_loop(rx));

    let callback_notify = Arc::new(tokio::sync::Notify::new());

    let downstream = Arc::new(Downstream {
        update_title_url: downstream_config.update_title_url,
        segment_url: downstream_config.segment_url,
    });

    let callback_notify_ = callback_notify.clone();

    tokio::spawn(async {
        let r = stream_handler(store_rx, downstream, stream_tx, llm_config, tts_config).await;
        if let Err(e) = r {
            log::error!("stream_handler error: {:?}", e);
        }
    });

    Router::new()
        .route("/send_msg_form", post(send_msg_form))
        .route("/callback", any(callback))
        .layer(Extension(tx))
        .layer(Extension(callback_notify_))
        .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024))
}

async fn callback(
    Extension(callback_notify): Extension<Arc<tokio::sync::Notify>>,
) -> Result<String, StatusCode> {
    log::info!("callback");
    callback_notify.notify_waiters();
    Ok("ok".to_string())
}

#[derive(Debug, serde::Deserialize)]
pub struct SendMsgRequest {
    vtb_name: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    motion: Option<String>,
    #[serde(skip)]
    voice: Option<Bytes>,
}

async fn parse_from_multipart(mut multipart: Multipart) -> anyhow::Result<SendMsgRequest> {
    let mut req = SendMsgRequest {
        vtb_name: "".to_string(),
        text: None,
        motion: None,
        voice: None,
    };

    while let Some(field) = multipart.next_field().await? {
        let field_name = field.name().unwrap_or_default();
        match field_name {
            "vtb_name" => {
                req.vtb_name = field.text().await?;
            }
            "text" => {
                req.text = Some(field.text().await?);
            }
            "motion" => {
                req.motion = Some(field.text().await?);
            }
            "voice" => {
                let data = field.bytes().await?;
                if !data.is_empty() {
                    req.voice = Some(data);
                }
            }
            _ => {}
        }
    }

    Ok(req)
}

async fn send_msg_form(
    Extension(tx): Extension<PodcastTx>,
    multipart: Multipart,
) -> Result<String, StatusCode> {
    let msg = parse_from_multipart(multipart).await;
    if let Err(e) = msg {
        log::error!("parse_from_multipart error: {:?}", e);
        return Err(StatusCode::BAD_REQUEST);
    }

    match tx.send(msg.unwrap()).await {
        Ok(_) => Ok(format!("ok")),
        Err(e) => {
            log::error!("random_say error: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

type PodcastTx = tokio::sync::mpsc::Sender<SendMsgRequest>;
type PodcastRx = tokio::sync::mpsc::Receiver<SendMsgRequest>;

pub struct Podcast {
    title: String,
    segment: Vec<SendMsgRequest>,
}

struct PodcastStore {
    store: tokio::sync::mpsc::UnboundedSender<Podcast>,
}

impl PodcastStore {
    fn new(store: tokio::sync::mpsc::UnboundedSender<Podcast>) -> Self {
        Self { store }
    }

    async fn run_loop(self, mut rx: PodcastRx) -> anyhow::Result<()> {
        let mut title_index = 0;

        loop {
            let mut podcast = Podcast {
                title: format!("title {}", title_index),
                segment: Vec::with_capacity(20),
            };

            let segment = rx.recv().await.ok_or(anyhow::anyhow!("rx closed"))?;
            podcast.segment.push(segment);

            loop {
                let segment =
                    tokio::time::timeout(std::time::Duration::from_secs(15), rx.recv()).await;
                match segment {
                    Ok(Some(segment)) => {
                        podcast.segment.push(segment);
                    }

                    Ok(None) => {
                        Err(anyhow::anyhow!("rx closed"))?;
                    }
                    Err(_) => {
                        title_index += 1;
                        self.store
                            .send(podcast)
                            .map_err(|_| anyhow::anyhow!("podcast out channel closed"))?;
                        break;
                    }
                }
            }
        }
    }
}

pub struct Downstream {
    pub update_title_url: String,
    pub segment_url: String,
}

impl Downstream {
    pub async fn update_title(&self, title: String) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let res = client
            .post(&self.update_title_url)
            .json(&serde_json::json!({"title": title}))
            .send()
            .await?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("update title failed"))
        }
    }

    pub async fn send_segment(
        &self,
        client: &reqwest::Client,
        segment: SendMsgRequest,
    ) -> anyhow::Result<()> {
        let SendMsgRequest {
            vtb_name,
            text,
            motion,
            voice,
        } = segment;
        let mut form = reqwest::multipart::Form::new().part("vtb_name", Part::text(vtb_name));
        if let Some(text) = text {
            form = form.part("text", Part::text(text));
        }
        if let Some(motion) = motion {
            form = form.part("motion", Part::text(motion));
        }
        if let Some(voice) = voice {
            form = form.part("voice", Part::stream(voice).file_name("audio.wav"));
        }

        let res = client
            .post(&self.segment_url)
            .multipart(form)
            .send()
            .await?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("send segment failed"))
        }
    }

    pub async fn send(&self, podcast: Podcast) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let Podcast { title, segment } = podcast;
        self.update_title(title).await?;
        for s in segment {
            self.send_segment(&client, s).await?;
        }

        Ok(())
    }
}

async fn get_comments(
    stream_tx: &tokio::sync::mpsc::UnboundedSender<CommentTx>,
) -> anyhow::Result<LinkedList<(String, String)>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    stream_tx
        .send(tx)
        .map_err(|_| anyhow::anyhow!("stream_tx closed"))?;
    rx.await
        .map_err(|_| anyhow::anyhow!("stream_tx closed on get_commonts"))
}

fn parse_comments(comments: LinkedList<(String, String)>) -> String {
    let mut text = String::new();
    text.push_str("以下是用户的评论：\n");
    for (user, content) in comments {
        text.push_str(&format!("{}: {}\n", user, content));
    }
    text
}

pub struct LlmAgent {
    pub downstream: Arc<Downstream>,
    pub tts_config: TTSConfig,
}

impl LlmAgent {
    pub async fn reply<I: IntoIterator<Item = C>, C: AsRef<Content>>(
        &mut self,
        llm_url: &str,
        token: &str,
        prompts: I,
    ) -> anyhow::Result<String> {
        let http_cli = reqwest::Client::new();
        let mut resp = llm_stable(llm_url, token, None, prompts)
            .await
            .map_err(|e| anyhow::anyhow!("llm_stable error: {:?}", e))?;
        let mut llm_reply = String::with_capacity(128);
        while let Some(chunk) = resp
            .next_chunk()
            .await
            .map_err(|e| anyhow::anyhow!("llm_stable next_chunk error: {:?}", e))?
        {
            llm_reply.push_str(&chunk);
            let (vtb_name, audio) = match &self.tts_config {
                TTSConfig::Stable(StableTTS {
                    base_url,
                    speaker,
                    vtb_name,
                }) => (
                    vtb_name.to_string(),
                    tts(base_url, speaker, &chunk)
                        .await
                        .map_err(|e| anyhow::anyhow!("tts error: {:?}", e)),
                ),
                TTSConfig::Fish(FishTTS {
                    api_key,
                    speaker,
                    vtb_name,
                }) => (
                    vtb_name.to_string(),
                    fish_tts(api_key, speaker, &chunk)
                        .await
                        .map_err(|e| anyhow::anyhow!("tts error: {:?}", e)),
                ),
            };

            if let Err(e) = &audio {
                log::error!("tts failed: {:?}", e);
            }

            let voice = audio.ok();

            if let Err(e) = self
                .downstream
                .send_segment(
                    &http_cli,
                    SendMsgRequest {
                        vtb_name,
                        text: Some(chunk),
                        motion: None,
                        voice,
                    },
                )
                .await
            {
                log::error!("send_segment failed: {:?}", e);
            }
        }
        Ok(llm_reply)
    }
}

async fn stream_handler(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Podcast>,
    downstream: Arc<Downstream>,
    stream_tx: tokio::sync::mpsc::UnboundedSender<CommentTx>,
    llm_config: LLMConfig,
    tts_config: TTSConfig,
) -> anyhow::Result<()> {
    let LLMConfig {
        llm_chat_url,
        api_key,
        sys_prompts,
        mut dynamic_prompts,
        history,
    } = llm_config;

    let mut llm_agent = LlmAgent {
        downstream: downstream.clone(),
        tts_config,
    };

    let token = if let Some(t) = api_key.as_ref() {
        format!("Bearer {}", t)
    } else {
        String::new()
    };
    log::info!("llm chat url: {}", llm_chat_url);
    log::info!("llm token: {}", token);

    // let mut podcast = rx
    //     .recv()
    //     .await
    //     .ok_or(anyhow::anyhow!("podcast rx closed"))?;

    let mut podcast: Option<Podcast> = None;

    'podcast: loop {
        if let Some(podcast) = podcast {
            let title = podcast.title.clone();

            log::info!("podcast start");
            let r = downstream.send(podcast).await;
            if let Err(e) = r {
                log::error!("send podcast {title} failed: {:?}", e);
            }

            log::info!("podcast done");
        }

        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(60 * 3);

        loop {
            log::info!("wait comments");
            let comments = tokio::time::timeout_at(timeout.into(), get_comments(&stream_tx)).await;
            let comments = if comments.is_err() {
                tokio::select! {
                    p = rx.recv() => {
                        if let Some(p) = p {
                            podcast = Some(p);
                            continue 'podcast;
                        } else {
                            return Err(anyhow::anyhow!("podcast rx closed"));
                        }
                    }
                    comments= get_comments(&stream_tx) => {
                        comments?
                    }
                }
            } else {
                comments.unwrap()?
            };
            log::info!("wait {} comments", comments.len());

            dynamic_prompts.push_back(Content {
                role: crate::llm::llm::Role::User,
                message: parse_comments(comments),
            });
            if dynamic_prompts.len() > history * 2 {
                dynamic_prompts.pop_front();
                dynamic_prompts.pop_front();
            }

            log::info!("llm_agent reply\n{:#?}", dynamic_prompts);
            match llm_agent
                .reply(
                    &llm_chat_url,
                    &token,
                    sys_prompts.iter().chain(dynamic_prompts.iter()),
                )
                .await
            {
                Ok(reply) => {
                    log::info!("llm_agent reply done");
                    dynamic_prompts.push_back(Content {
                        role: crate::llm::llm::Role::Assistant,
                        message: reply,
                    });
                }
                Err(e) => {
                    log::error!("llm_agent reply failed: {:?}", e);
                }
            }
        }
    }
}
