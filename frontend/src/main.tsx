import { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./styles.css";

type TrackKind = "Video" | "Audio";
type Profile = { smoothness: number; strength: number; horizon_lock: boolean; dynamic_fov: number };
type Clip = { id: string; source_path: string; in_point: number; out_point: number; position: number; stabilization?: Profile | null; lut_path?: string | null; speed_keyframes: unknown[]; text_overlays: unknown[]; osd_source?: string | null };
type Track = { id: string; kind: TrackKind; name: string; clip_order: string[] };
type Project = { name: string; fps: number; width: number; height: number; tracks: Track[]; clips: Record<string, Clip> };
type Outcome = { project: Project; can_undo: boolean; can_redo: boolean };

const fmt = (us: number) => `${Math.floor(us / 60_000_000)}:${String(Math.floor(us / 1_000_000) % 60).padStart(2, "0")}`;
const fileName = (path: string) => path.split(/[\\/]/).pop() || path;
const seedProject: Project = { name: "Untitled", fps: 60, width: 1920, height: 1080, tracks: [], clips: {} };

function App() {
  const [project, setProject] = useState<Project>(seedProject);
  const [selected, setSelected] = useState<string>();
  const [playhead, setPlayhead] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [undoable, setUndoable] = useState(false);
  const [redoable, setRedoable] = useState(false);
  const [notice, setNotice] = useState("READY FOR TAKEOFF");
  const [assistantOpen, setAssistantOpen] = useState(true);
  const [prompt, setPrompt] = useState("");
  const [messages, setMessages] = useState<{ role: "you" | "ai"; text: string }[]>([{ role: "ai", text: "I’m wired into your timeline. Ask me to cut, stabilize, or shape this run." }]);

  const selectedClip = selected ? project.clips[selected] : undefined;
  const duration = useMemo(() => Math.max(30_000_000, ...Object.values(project.clips).map(c => c.position + c.out_point - c.in_point)), [project]);
  const apply = (outcome: Outcome) => { setProject(outcome.project); setUndoable(outcome.can_undo); setRedoable(outcome.can_redo); };
  const call = async (command: object) => { try { apply(await invoke<Outcome>("execute", { command })); } catch (e) { setNotice(`ERROR · ${String(e)}`); } };
  const refresh = async () => { try { setProject(await invoke<Project>("timeline")); } catch { setNotice("BROWSER PREVIEW · TAURI IPC OFFLINE"); } };
  useEffect(() => { void refresh(); }, []);
  useEffect(() => { if (!playing) return; const id = window.setInterval(() => setPlayhead(p => p >= duration ? 0 : p + 100_000), 100); return () => window.clearInterval(id); }, [playing, duration]);

  const addTrack = async (kind: TrackKind) => call({ command: "add_track", kind, name: kind === "Video" ? `V${project.tracks.filter(t => t.kind === "Video").length + 1}` : `A${project.tracks.filter(t => t.kind === "Audio").length + 1}` });
  const importMedia = async () => {
    const path = await open({ multiple: false, filters: [{ name: "Video", extensions: ["mp4", "mov", "mkv", "m4v"] }] });
    if (!path || Array.isArray(path)) return;
    let track = project.tracks.find(t => t.kind === "Video");
    if (!track) { await addTrack("Video"); const state = await invoke<Project>("timeline"); setProject(state); track = state.tracks.find(t => t.kind === "Video"); }
    if (track) { await call({ command: "add_clip", track_id: track.id, clip: { source_path: path, in_point: 0, out_point: 10_000_000, position: duration > 30_000_000 ? duration : 0 } }); setNotice(`IMPORTED · ${fileName(path)}`); }
  };
  const saveProject = async () => { const path = await save({ defaultPath: `${project.name || "flight"}.fpv.json`, filters: [{ name: "FPV Project", extensions: ["fpv.json"] }] }); if (path) { await invoke("save_project", { path }); setNotice("PROJECT SAVED"); } };
  const loadProject = async () => { const path = await open({ multiple: false, filters: [{ name: "FPV Project", extensions: ["json"] }] }); if (typeof path === "string") { setProject(await invoke<Project>("load_project", { path })); setSelected(undefined); setNotice("PROJECT LOADED"); } };
  const sendPrompt = async () => { if (!prompt.trim()) return; const text = prompt; setPrompt(""); setMessages(m => [...m, { role: "you", text }]); try { const reply = await invoke<string>("chat", { prompt: text }); setMessages(m => [...m, { role: "ai", text: reply }]); await refresh(); } catch (e) { setMessages(m => [...m, { role: "ai", text: `Connect an AI provider in settings first. (${String(e)})` }]); } };

  return <main className="app-shell">
    <header className="topbar"><div className="brand"><span className="brand-mark">⟐</span><div><strong>APEX</strong><small>FPV EDITOR</small></div></div><div className="project-title"><span className="status-dot" /> {project.name} <em>· {project.width}×{project.height} / {project.fps} FPS</em></div><div className="top-actions"><button onClick={() => void loadProject()}>OPEN</button><button onClick={() => void saveProject()}>SAVE</button><button className="export" onClick={() => setNotice("EXPORT PRESET PANEL — COMING THROUGH THE RENDER PIPELINE")}>EXPORT <span>↗</span></button></div></header>
    <section className="toolbar"><button onClick={() => void importMedia()} className="accent">＋ MEDIA</button><i /><button disabled={!undoable} onClick={() => invoke<Outcome>("undo").then(apply)}>↶</button><button disabled={!redoable} onClick={() => invoke<Outcome>("redo").then(apply)}>↷</button><span className="notice">{notice}</span><button onClick={() => setAssistantOpen(v => !v)} className={assistantOpen ? "active" : ""}>✦ COPILOT</button></section>
    <section className={`workbench ${assistantOpen ? "with-ai" : ""}`}>
      <aside className="media-bin"><div className="panel-heading"><span>MEDIA BIN</span><button onClick={() => void importMedia()}>＋</button></div><div className="media-empty"><div className="drone-glyph">⌁</div><b>Build your flight</b><p>Import clips, then cut the rush into a line.</p><button onClick={() => void importMedia()}>IMPORT MEDIA</button></div><div className="bin-items">{Object.values(project.clips).map(c => <button key={c.id} className={selected === c.id ? "bin-item selected" : "bin-item"} onClick={() => setSelected(c.id)}><span className="thumbnail">▣</span><span>{fileName(c.source_path)}<small>{fmt(c.out_point - c.in_point)}</small></span></button>)}</div></aside>
      <section className="center"><div className="preview"><div className="safe-frame"><div className="horizon" /><div className="reticle">+</div><div className="preview-copy"><span>NO SIGNAL</span><b>SELECT A CLIP TO PREVIEW</b><small>VIDEO MONITOR · REC 709</small></div><div className="telemetry"><span>00: {fmt(playhead)}</span><span>● 60 FPS</span></div></div></div><div className="transport"><span>{fmt(playhead)}</span><div className="transport-controls"><button onClick={() => setPlayhead(Math.max(0, playhead - 1_000_000))}>|◀</button><button className="play" onClick={() => setPlaying(v => !v)}>{playing ? "Ⅱ" : "▶"}</button><button onClick={() => setPlayhead(Math.min(duration, playhead + 1_000_000))}>▶|</button></div><span>{fmt(duration)}</span></div></section>
      <aside className="inspector"><div className="panel-heading"><span>INSPECTOR</span><button>···</button></div>{selectedClip ? <ClipInspector clip={selectedClip} run={call} /> : <div className="inspector-empty"><span>◇</span><b>Nothing selected</b><p>Pick a clip in the timeline to tune its flight character.</p></div>}</aside>
      {assistantOpen && <aside className="copilot"><div className="panel-heading"><span><i className="spark">✦</i> FLIGHT COPILOT</span><button onClick={() => setAssistantOpen(false)}>×</button></div><div className="chat"><div className="ai-orb">✦</div>{messages.map((m, i) => <div className={`message ${m.role}`} key={i}>{m.text}</div>)}</div><div className="suggestions"><button onClick={() => setPrompt("Stabilize the selected clip with horizon lock")}>Stabilize this take</button><button onClick={() => setPrompt("Find the best moments for a fast freestyle cut")}>Find the best moments</button></div><div className="prompt"><textarea value={prompt} onChange={e => setPrompt(e.target.value)} onKeyDown={e => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); void sendPrompt(); } }} placeholder="Tell Copilot what to make…" /><button onClick={() => void sendPrompt()}>↑</button></div></aside>}
    </section>
    <section className="timeline"><div className="timeline-header"><div className="timeline-label">TIMELINE <small>MAGNETIC</small></div><div className="ruler">{[0, 5, 10, 15, 20, 25, 30].map(s => <span key={s} style={{ left: `${s / 30 * 100}%` }}>{`00:${String(s).padStart(2, "0")}`}</span>)}</div><div className="timeline-tools"><button onClick={() => void addTrack("Video")}>＋ VIDEO</button><button onClick={() => void addTrack("Audio")}>＋ AUDIO</button></div></div><div className="tracks">{project.tracks.length === 0 && <div className="timeline-empty">Drop into the timeline · or use <b>＋ MEDIA</b> to begin</div>}{project.tracks.map(track => <div className="track" key={track.id}><div className="track-name"><b>{track.name}</b><small>{track.kind === "Video" ? "VIDEO" : "AUDIO"}</small></div><div className="track-lane">{track.clip_order.map(id => { const c = project.clips[id]; if (!c) return null; const left = c.position / duration * 100; const width = Math.max(8, (c.out_point - c.in_point) / duration * 100); return <button onClick={() => setSelected(id)} key={id} className={`clip ${selected === id ? "selected" : ""}`} style={{ left: `${left}%`, width: `${width}%` }}><span>⌁</span>{fileName(c.source_path)}{c.stabilization && <em>STAB</em>}</button>; })}</div></div>)}</div><div className="playhead" style={{ left: `calc(148px + ${(playhead / duration) * 100}% * (100% - 148px) / 100)` }} /></section>
  </main>;
}

function ClipInspector({ clip, run }: { clip: Clip; run: (command: object) => Promise<void> }) {
  const [smoothness, setSmoothness] = useState(clip.stabilization?.smoothness ?? .55);
  const profile = () => ({ smoothness, strength: clip.stabilization?.strength ?? 1, horizon_lock: clip.stabilization?.horizon_lock ?? true, dynamic_fov: clip.stabilization?.dynamic_fov ?? .12 });
  return <div className="inspector-content"><div className="selected-title"><span>▣</span><div><b>{fileName(clip.source_path)}</b><small>{fmt(clip.out_point - clip.in_point)} · 4K SOURCE</small></div></div><label>STABILIZATION <small>{clip.stabilization ? "ACTIVE" : "OFF"}</small></label><div className="toggle-row"><span>Gyroflow engine</span><button className={clip.stabilization ? "toggle on" : "toggle"} onClick={() => void run({ command: "apply_stabilization", clip_id: clip.id, profile: profile() })}><i /></button></div><div className="range-row"><span>Smoothness</span><output>{Math.round(smoothness * 100)}%</output></div><input type="range" min="0" max="1" step=".05" value={smoothness} onChange={e => setSmoothness(+e.target.value)} onMouseUp={() => void run({ command: "apply_stabilization", clip_id: clip.id, profile: profile() })} /><div className="toggle-row"><span>Horizon lock</span><button className="toggle on" onClick={() => void run({ command: "apply_stabilization", clip_id: clip.id, profile: { ...profile(), horizon_lock: !profile().horizon_lock } })}><i /></button></div><div className="divider" /><label>COLOR</label><button className="select-control" onClick={() => void run({ command: "apply_lut", clip_id: clip.id, lut_path: "cinematic-teal.cube" })}>Cinematic teal <span>⌄</span></button><label>SPEED RAMP</label><div className="speed-buttons"><button>0.5×</button><button className="chosen">1×</button><button>2×</button></div><button className="danger" onClick={() => void run({ command: "remove_clip", clip_id: clip.id })}>REMOVE FROM TIMELINE</button></div>
}

createRoot(document.getElementById("root")!).render(<App />);
