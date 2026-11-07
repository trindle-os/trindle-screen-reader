//! Test helper: wait for a window to be activated, then move the focus to the first object
use atspi_connection::AccessibilityConnection;
use atspi::events::window::ActivateEvent;
use atspi::events::{Event, WindowEvents};
use atspi::proxy::accessible::ObjectRefExt;
use atspi::proxy::component::ComponentProxy;
use atspi::proxy::editable_text::EditableTextProxy;
use atspi::proxy::text::TextProxy;
use futures_lite::StreamExt;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let obj = std::env::args().nth(1).unwrap_or_default().to_lowercase();
    let conn = AccessibilityConnection::new().await.unwrap();
    atspi_connection::set_session_accessibility(true).await.unwrap();
    conn.register_event::<ActivateEvent>().await.unwrap();
    let mut stream = conn.event_stream();
    let win = tokio::time::timeout(Duration::from_secs(8), async {
        while let Some(Ok(e)) = stream.next().await {
            if let Event::Window(WindowEvents::Activate(a)) = e {
                return Some(a.item);
            }
        }
        None
    })
    .await
    .ok()
    .flatten();
    let Some(win) = win else { return println!("no window activated") };
    let mut stack = vec![win];
    let mut seen = 0;
    while let Some(r) = stack.pop() {
        seen += 1;
        if seen > 5000 {
            break;
        }
        let Ok(x) = r.as_accessible_proxy(conn.connection()).await else { continue };
        let name = x.name().await.unwrap_or_default().to_lowercase();
        if !name.is_empty() && name == obj {
            let c = ComponentProxy::builder(conn.connection())
                .destination(r.name_as_str().unwrap()).unwrap()
                .path(r.path_as_str()).unwrap().build().await.unwrap();
            println!("focus {name}: {:?}", c.grab_focus().await);
            if let Some(words) = std::env::args().nth(2) {
                let (d, p) = (r.name_as_str().unwrap(), r.path_as_str());
                let ed = EditableTextProxy::builder(conn.connection()).destination(d).unwrap().path(p).unwrap().build().await.unwrap();
                let tx = TextProxy::builder(conn.connection()).destination(d).unwrap().path(p).unwrap().build().await.unwrap();
                tokio::time::sleep(Duration::from_millis(800)).await;
                for (i, ch) in words.chars().enumerate() {
                    println!("insert {ch:?}: {:?}", ed.insert_text(i as i32, &ch.to_string(), 1).await);
                    tokio::time::sleep(Duration::from_millis(400)).await;
                }
                for at in [0, 1, 2] {
                    println!("caret {at}: {:?}", tx.set_caret_offset(at).await);
                    tokio::time::sleep(Duration::from_millis(400)).await;
                }
                println!("delete: {:?}", ed.delete_text(0, 1).await);
                tokio::time::sleep(Duration::from_millis(600)).await;
            }
            return;
        }
        stack.extend(x.get_children().await.unwrap_or_default());
    }
    println!("not found");
}
