use std::{path::Path, sync::Arc};

use notify::{RecursiveMode, Watcher};
use tokio::{
    process::Command,
    select,
    sync::{watch, Notify},
};
use tokio_util::sync::CancellationToken;

pub async fn get_active_user_count() -> Result<usize, std::io::Error> {
    let users_command_output = Command::new("users").output().await?;
    let users_output = String::from_utf8(users_command_output.stdout).unwrap();
    let user_count = users_output.split_whitespace().count();
    Ok(user_count)
}

pub async fn watch_active_user_count(
    stop: CancellationToken,
) -> tokio::sync::watch::Receiver<usize> {
    let (tx, user_count_rx) = watch::channel(get_active_user_count().await.unwrap());

    tokio::spawn(user_count_update_loop(stop, tx));
    println!("Select loop quit");
    return user_count_rx;
}

async fn user_count_update_loop(stop: CancellationToken, user_count: watch::Sender<usize>) {
    let query_user_count_signal = Arc::new(Notify::new());
    let notify_query_user_count = query_user_count_signal.clone();

    let _watcher = create_user_count_update_notifier(notify_query_user_count);

    loop {
        select! {
            _ = query_user_count_signal.notified() => {
                let new_user_count = get_active_user_count().await.unwrap();
                match user_count.send(new_user_count) {
                    Err(_) => break,
                    _ => {},
                }
            },
            _ = stop.cancelled() =>  {
                break;
            }
        }
    }
}

fn create_user_count_update_notifier(notify: Arc<Notify>) -> impl Watcher {
    let mut watcher =
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| match res {
            Ok(event) => {
                if event.kind.is_modify() {
                    notify.notify_one();
                }
            }
            Err(_) => todo!(),
        })
        .unwrap();

    watcher
        .watch(Path::new("/var/run/utmp"), RecursiveMode::NonRecursive)
        .unwrap();

    watcher
}
