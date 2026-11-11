//! Live check: wait for a focus event, then print what review and browse mode would gather and say.
use reader::browse::{Browser, Kind};
use reader::bus::Bus;
use reader::model::Event;
use std::rc::Rc;
use std::time::Duration;

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async {
        let bus = Rc::new(Bus::connect().await.unwrap());
        let mut rx = bus.clone().events();
        let got = tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(e) = rx.recv().await {
                if let Event::Focus { object } = e {
                    return Some(object);
                }
            }
            None
        })
        .await;
        println!("focus: {:?}", got.ok().flatten().map(|o| o.name));
        tokio::time::sleep(Duration::from_millis(500)).await;
        match bus.window_items().await {
            Some((items, focus, _)) => {
                println!("items: {} focus id {:?}", items.len(), focus);
                for i in &items {
                    println!("  #{} {:?} {:?} text={:?} at {},{} {}x{}", i.id, i.role, i.name, i.text, i.x, i.y, i.w, i.h);
                }
            }
            None => println!("no items"),
        }
        println!("focused text: {:?}", bus.focused_text().await);
        match bus.document_nodes().await {
            Some((nodes, refs, focus, _)) => {
                println!("browse nodes: {} focus at {:?}", nodes.len(), focus);
                for n in &nodes {
                    println!("  {}{:?} {:?} text={:?} level={} count={}", "  ".repeat(n.depth as usize), n.role, n.name, n.text, n.level, n.count);
                }
                let mut b = Browser::new(nodes, focus);
                println!("read: {:?}", b.read());
                println!("next: {:?}", b.step(true));
                println!("next: {:?}", b.step(true));
                println!("next: {:?}", b.step(true));
                println!("field: {:?}", b.jump(Kind::Field, true));
                println!("heading: {:?}", b.jump(Kind::Heading, true));
                println!("landmark: {:?}", b.jump(Kind::Landmark, true));
                println!("field back: {:?}", b.jump(Kind::Field, false));
                if let Some(i) = b.focusable_cursor() {
                    println!("grab focus of node {i}: {}", bus.grab_focus(&refs[i]).await);
                }
                println!("fields: {:?}", b.elements(Kind::Field));
                println!("mode: {:?}", b.toggle_mode());
            }
            None => println!("no document"),
        }
        bus.finish().await;
    });
}
