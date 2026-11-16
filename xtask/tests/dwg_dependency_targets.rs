//! One acadrust across the workspace: the IO crate's tests inspect DWGs with the decoder version
//! `cadcraft-dwg` reads and writes them with (DWG goes through `cadcraft-dwg` on every target,
//! WebAssembly included, so the tests' dependency has no target of its own).

use serde_json::Value;
use std::process::Command;

#[test]
fn dwg_test_dependency_matches_the_shared_bridge_targets() {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("read workspace dependency metadata");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let metadata: Value = serde_json::from_slice(&output.stdout).expect("Cargo returns JSON");
    let packages = metadata["packages"].as_array().expect("workspace packages");
    let dependency = |name: &str| {
        let package = packages.iter().find(|p| p["name"] == name).expect("workspace crate");
        package["dependencies"].as_array().expect("crate dependencies").iter().find(|d| d["name"] == "acadrust").expect("DWG decoder dependency").clone()
    };
    let tests = dependency("cadcraft-io");
    let dwg = dependency("cadcraft-dwg");
    assert_eq!(tests["kind"], "dev", "the IO crate only inspects acadrust output in tests");
    assert_eq!(tests["req"], dwg["req"], "tests inspect output from the decoder version cadcraft-dwg uses");
    assert_eq!(dwg["kind"], Value::Null, "cadcraft-dwg reads and writes DWG with acadrust");
}
