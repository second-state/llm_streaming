use std::collections::LinkedList;

pub mod bilibili;
pub mod restream;

pub enum SteamEvent {
    Comment { user: String, content: String },
}

pub trait StreamPlatform {
    async fn next_event(&mut self) -> anyhow::Result<SteamEvent>;
}

pub type CommentTx = tokio::sync::oneshot::Sender<LinkedList<(String, String)>>;
#[allow(unused)]
pub type CommentRx = tokio::sync::oneshot::Receiver<LinkedList<(String, String)>>;

pub async fn llm_loop<P: StreamPlatform>(
    max_comment: usize,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<CommentTx>,
    mut platform: P,
) -> anyhow::Result<()> {
    let mut comment_store = LinkedList::new();

    loop {
        let r = tokio::select! {
            tx = rx.recv()=>{
                Ok(tx)
            }
            comment = platform.next_event() =>{
                Err(comment)
            }
        };
        match r {
            Ok(Some(tx)) => {
                if comment_store.is_empty() {
                    log::info!("no comment, wait for platform");
                    let SteamEvent::Comment { user, content } = platform
                        .next_event()
                        .await
                        .map_err(|e| anyhow::anyhow!("platform error: {:?}", e))?;

                    log::info!("comment: {} -> {}", user, content);
                    let mut new_comment = LinkedList::new();
                    new_comment.push_back((user, content));
                    let _ = tx.send(new_comment);
                } else {
                    let mut new_comment = LinkedList::new();
                    std::mem::swap(&mut new_comment, &mut comment_store);
                    let _ = tx.send(new_comment);
                }
            }

            Err(Ok(SteamEvent::Comment { user, content })) => {
                log::info!("comment: {} -> {}", user, content);
                comment_store.push_back((user, content));
                if comment_store.len() > max_comment {
                    comment_store.pop_front();
                }
            }
            Ok(None) => {
                return Err(anyhow::anyhow!("platform rx closed"));
            }
            Err(Err(e)) => {
                return Err(anyhow::anyhow!("platform error: {:?}", e));
            }
        }
    }
}
