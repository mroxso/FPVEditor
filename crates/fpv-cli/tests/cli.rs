//! End-to-end tests driving the real `fpv` binary as a subprocess, the way
//! a shell script or CI pipeline would (PLAN.md section 2's "scripting"
//! use case).

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn fpv() -> Command {
    Command::cargo_bin("fpv").unwrap()
}

fn temp_project_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("fpv-cli-test-{tag}-{}.fpv.json", std::process::id()))
}

fn json_stdout(cmd: &mut Command) -> Value {
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON")
}

#[test]
fn full_editing_workflow_via_the_cli_persists_correctly_between_invocations() {
    let path = temp_project_path("workflow");
    let _cleanup = CleanupOnDrop(path.clone());

    // new
    let project = json_stdout(fpv().args(["new", path.to_str().unwrap(), "--name", "My Edit"]));
    assert_eq!(project["name"], "My Edit");
    assert!(project["tracks"].as_array().unwrap().is_empty());

    // add-track
    let project = json_stdout(fpv().args([
        "add-track",
        path.to_str().unwrap(),
        "--kind",
        "video",
        "--name",
        "V1",
    ]));
    let track_id = project["tracks"][0]["id"].as_str().unwrap().to_string();

    // add-clip
    let project = json_stdout(fpv().args([
        "add-clip",
        path.to_str().unwrap(),
        "--track",
        &track_id,
        "--source",
        "run.mp4",
        "--in",
        "0",
        "--out",
        "10",
    ]));
    let clips = project["clips"].as_object().unwrap();
    assert_eq!(clips.len(), 1);
    let clip_id = clips.keys().next().unwrap().clone();

    // trim-clip
    let project = json_stdout(fpv().args([
        "trim-clip",
        path.to_str().unwrap(),
        "--clip",
        &clip_id,
        "--in",
        "1",
        "--out",
        "8",
    ]));
    let clip = &project["clips"][&clip_id];
    assert_eq!(clip["in_point"], 1_000_000);
    assert_eq!(clip["out_point"], 8_000_000);

    // stabilize
    let project = json_stdout(fpv().args([
        "stabilize",
        path.to_str().unwrap(),
        "--clip",
        &clip_id,
        "--smoothness",
        "0.7",
        "--strength",
        "1.0",
        "--horizon-lock",
        "--dynamic-fov",
        "0.2",
    ]));
    let stab = &project["clips"][&clip_id]["stabilization"];
    assert_eq!(stab["horizon_lock"], true);
    assert!((stab["smoothness"].as_f64().unwrap() - 0.7).abs() < 1e-6);

    // apply-lut
    let project = json_stdout(fpv().args([
        "apply-lut",
        path.to_str().unwrap(),
        "--clip",
        &clip_id,
        "--lut",
        "warm.cube",
    ]));
    assert_eq!(project["clips"][&clip_id]["lut_path"], "warm.cube");

    // split-clip at t=4s (within the trimmed [1,8) range on the timeline)
    let project = json_stdout(fpv().args([
        "split-clip",
        path.to_str().unwrap(),
        "--clip",
        &clip_id,
        "--at",
        "4",
    ]));
    assert_eq!(project["clips"].as_object().unwrap().len(), 2);

    // list reflects what's on disk
    let list = json_stdout(fpv().args(["list", path.to_str().unwrap()]));
    assert_eq!(list.as_array().unwrap().len(), 2);

    // show round-trips the same state list produced
    let shown = json_stdout(fpv().args(["show", path.to_str().unwrap()]));
    assert_eq!(shown["clips"].as_object().unwrap().len(), 2);
}

#[test]
fn add_clip_against_a_nonexistent_track_fails_without_corrupting_the_project_file() {
    let path = temp_project_path("bad-track");
    let _cleanup = CleanupOnDrop(path.clone());

    let before = json_stdout(fpv().args(["new", path.to_str().unwrap()]));

    let output = fpv()
        .args([
            "add-clip",
            path.to_str().unwrap(),
            "--track",
            &uuid::Uuid::new_v4().to_string(),
            "--source",
            "x.mp4",
            "--in",
            "0",
            "--out",
            "1",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());

    let after = json_stdout(fpv().args(["show", path.to_str().unwrap()]));
    assert_eq!(before, after, "a failed command must not modify the saved project");
}

#[test]
fn creating_a_project_at_an_existing_path_is_refused() {
    let path = temp_project_path("exists");
    let _cleanup = CleanupOnDrop(path.clone());

    fpv().args(["new", path.to_str().unwrap()]).assert().success();
    fpv().args(["new", path.to_str().unwrap()]).assert().failure();
}

#[test]
fn probing_a_synthetic_ffmpeg_generated_file_reports_expected_dimensions() {
    if std::process::Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("skipping: ffmpeg not available on PATH");
        return;
    }
    let dir = std::env::temp_dir().join(format!("fpv-cli-probe-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("src.mp4");
    std::process::Command::new("ffmpeg")
        .args([
            "-y", "-f", "lavfi", "-i", "testsrc=size=160x120:rate=24:duration=1",
            "-pix_fmt", "yuv420p", source.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let info = json_stdout(fpv().args(["probe", source.to_str().unwrap()]));
    assert_eq!(info["width"], 160);
    assert_eq!(info["height"], 120);

    std::fs::remove_dir_all(&dir).ok();
}

struct CleanupOnDrop(PathBuf);
impl Drop for CleanupOnDrop {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}
