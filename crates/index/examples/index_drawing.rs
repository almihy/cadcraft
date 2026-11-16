//! Index a drawing and ask it the agent's questions, with timings:
//! `cargo run --release -p cadcraft-index --example index_drawing -- plan.dwg [out-dir]`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::time::Instant;

use cadcraft_index::{Opened, Query};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("a DWG or DXF to read");
    let out = args.get(2).map(std::path::PathBuf::from);
    let bytes = std::fs::read(path).expect("the file");
    let t = Instant::now();
    let d = cadcraft_index::read(&bytes, path).expect("read");
    println!("read: {:.2} s, {} model items", t.elapsed().as_secs_f64(), d.model.len());
    let t = Instant::now();
    let o = Opened::new(d);
    println!("opened (boxes): {:.2} s", t.elapsed().as_secs_f64());
    let t = Instant::now();
    let summary = o.summary();
    let text = serde_json::to_string(&summary).unwrap();
    println!("summary: {:.3} s, {} chars", t.elapsed().as_secs_f64(), text.len());
    let t = Instant::now();
    let packed = cadcraft_index::pack(&o.drawing).unwrap();
    println!("pack: {:.2} s, {:.1} MB (file {:.1} MB)", t.elapsed().as_secs_f64(), packed.len() as f64 / 1e6, bytes.len() as f64 / 1e6);
    drop(o);
    let t = Instant::now();
    let o = Opened::unpack(&packed).unwrap();
    println!("unpack + open: {:.2} s, {} model items", t.elapsed().as_secs_f64(), o.drawing.model.len());
    assert_eq!(serde_json::to_string(&o.summary()).unwrap(), text, "the summary is the same from the pack");
    let ask = |q: serde_json::Value| {
        let t = Instant::now();
        let q: Query = serde_json::from_value(q.clone()).unwrap();
        let got = o.query(&q);
        let el = t.elapsed().as_secs_f64();
        match got {
            Ok(v) => {
                let s = serde_json::to_string(&v).unwrap();
                println!("query {q:?}: {el:.3} s, total {}, next {}, {} chars", v["total"], v["next"], s.len());
                v
            }
            Err(e) => {
                println!("query {q:?}: {el:.3} s, error: {e}");
                serde_json::Value::Null
            }
        }
    };
    let layer = summary["layers"][0]["name"].as_str().unwrap_or("0").to_string();
    let p = ask(serde_json::json!({"layers": [layer], "limit": 3}));
    println!("  {}", serde_json::to_string(&p["items"]).unwrap());
    let p = ask(serde_json::json!({"kinds": ["block"], "limit": 2}));
    println!("  {}", serde_json::to_string(&p["items"]).unwrap());
    let p = ask(serde_json::json!({"kinds": ["text"], "limit": 3}));
    println!("  {}", serde_json::to_string(&p["items"]).unwrap());
    if let Some(ext) = summary["extents"].as_array() {
        let (a, b) = (&ext[0], &ext[1]);
        let (x0, y0, x1, y1) = (a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), b[0].as_f64().unwrap(), b[1].as_f64().unwrap());
        let mid = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
        let w = (x1 - x0) / 20.0;
        ask(serde_json::json!({"box": [[mid[0] - w, mid[1] - w], [mid[0] + w, mid[1] + w]], "limit": 50}));
    }
    ask(serde_json::json!({"text": "a", "limit": 5}));
    let p = ask(serde_json::json!({"kinds": ["blocks"], "start": 200, "limit": 100}));
    println!("  page 3 of blocks: start {}, next {}", p["start"], p["next"]);
    let top = summary["blocks"][0]["name"].as_str().unwrap_or("").to_string();
    let p = ask(serde_json::json!({"block": top, "limit": 5}));
    println!("  block {top}: {}", serde_json::to_string(&p).unwrap().chars().take(600).collect::<String>());
    let t = Instant::now();
    let flat = o.shapes(Some(&[layer.clone()]), None, true, 100_000);
    println!("shapes on {layer} + texts: {:.2} s, {:?}", t.elapsed().as_secs_f64(), flat.as_ref().map(Vec::len));
    for page in 1..=o.pages().min(3) {
        let t = Instant::now();
        let pic = o.picture(page, None, 2000).unwrap();
        println!("picture page {page}/{} ({}): {:.2} s, {}x{}, {:.2} MB", pic.pages, pic.name, t.elapsed().as_secs_f64(), pic.width, pic.height, pic.png.len() as f64 / 1e6);
        if let Some(dir) = &out {
            std::fs::write(dir.join(format!("page{page}.png")), &pic.png).unwrap();
        }
    }
    let t = Instant::now();
    let pic = o.picture(1, Some([0.4, 0.4, 0.6, 0.6]), 2000).unwrap();
    println!("picture of a part: {:.2} s, {}x{}", t.elapsed().as_secs_f64(), pic.width, pic.height);
    if let Some(dir) = &out {
        std::fs::write(dir.join("part.png"), &pic.png).unwrap();
        std::fs::write(dir.join("summary.json"), serde_json::to_string_pretty(&summary).unwrap()).unwrap();
    }
}
