import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Circle,
  Clapperboard,
  Command,
  Download,
  Film,
  FolderInput,
  FolderOpen,
  GripHorizontal,
  Info,
  Layers2,
  Pause,
  Play,
  Plus,
  RefreshCw,
  RotateCw,
  Scissors,
  Send,
  Settings,
  MousePointer2,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  Undo2,
  Upload,
  Video,
  WandSparkles,
  X,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import fpvEditorLogo from "@/assets/fpv-editor-logo.svg";
import "./styles.css";

type TrackKind = "Video" | "Audio";
type Profile = {
  smoothness: number;
  strength: number;
  horizon_lock: boolean;
  dynamic_fov: number;
};
type Clip = {
  id: string;
  source_path: string;
  in_point: number;
  out_point: number;
  position: number;
  stabilization?: Profile | null;
  lut_path?: string | null;
};
type Track = {
  id: string;
  kind: TrackKind;
  name: string;
  clip_order: string[];
};
type Project = {
  name: string;
  fps: number;
  width: number;
  height: number;
  tracks: Track[];
  clips: Record<string, Clip>;
};
type Outcome = { project: Project; can_undo: boolean; can_redo: boolean };
type MediaImportOutcome = Outcome & {
  imported_paths: string[];
  skipped_paths: string[];
};
type Provider = {
  base_url: string;
  api_key: string | null;
  model: string;
  extra_headers: Record<string, string>;
};
type TimelineGrid = "off" | "coarse" | "medium" | "fine";
type EditorPreferences = { mediaOpen: boolean; inspectorOpen: boolean; copilotOpen: boolean; timelineHeight: number; autoCheckUpdates: boolean; timelineGrid: TimelineGrid };
type RecentProject = { path: string; name: string; openedAt: number };
type UpdateCheckResult = {
  current_version: string;
  latest_version: string;
  update_available: boolean;
  release_url: string;
  release_notes: string | null;
  published_at: string | null;
  download_url: string | null;
  asset_name: string | null;
};
type ToolDiagnostic = {
  binary: string;
  path: string;
  source: string;
  status: "healthy" | "missing" | "error";
  version: string | null;
  message: string;
};
type MediaDiagnostics = { ffmpeg: ToolDiagnostic; ffprobe: ToolDiagnostic };
type WorkflowPhase = "import" | "stabilize" | "cut" | "grade" | "export";
type EditTool = "select" | "razor";
type PreviewMode = "clip" | "timeline";
const workflowPhases: {
  id: WorkflowPhase;
  label: string;
  eyebrow: string;
  description: string;
  icon: typeof FolderInput;
}[] = [
  { id: "import", label: "Import", eyebrow: "01 · Media", description: "Link your sources and choose the takes worth keeping.", icon: FolderInput },
  { id: "stabilize", label: "Stabilize", eyebrow: "02 · Motion", description: "Tune horizon lock and motion handling for each take.", icon: SlidersHorizontal },
  { id: "cut", label: "Edit", eyebrow: "03 · Timeline", description: "Arrange moments, trim clips, and set the rhythm.", icon: Scissors },
  { id: "grade", label: "Look", eyebrow: "04 · Color", description: "Apply LUTs and define the final image treatment.", icon: Film },
  { id: "export", label: "Export", eyebrow: "05 · Delivery", description: "Review the timeline and prepare the final flight cut.", icon: Download },
];
const preferencesKey = "fpv-editor-preferences";
const recentProjectsKey = "fpv-editor-recent-projects";
const defaultPreferences: EditorPreferences = { mediaOpen: true, inspectorOpen: true, copilotOpen: true, timelineHeight: 220, autoCheckUpdates: true, timelineGrid: "medium" };
const readLocal = <T,>(key: string, fallback: T): T => {
  try { return JSON.parse(localStorage.getItem(key) ?? "") as T; } catch { return fallback; }
};
const seedProject: Project = {
  name: "Untitled",
  fps: 60,
  width: 1920,
  height: 1080,
  tracks: [],
  clips: {},
};
const defaultProvider: Provider = {
  base_url: "http://localhost:11434/v1",
  api_key: null,
  model: "",
  extra_headers: {},
};
const timecode = (us: number) =>
  `${Math.floor(us / 60_000_000)}:${String(Math.floor(us / 1_000_000) % 60).padStart(2, "0")}`;
const fileName = (path: string) => path.split(/[\\/]/).pop() || path;
const mediaSource = (path: string) => {
  try {
    return convertFileSrc(path);
  } catch {
    return undefined;
  }
};

function IconButton({
  label,
  children,
  ...props
}: React.ComponentProps<typeof Button> & { label: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button aria-label={label} size="icon-sm" variant="ghost" {...props}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

function ProjectLauncher({
  recentProjects,
  openRecent,
  removeRecent,
  openProject,
  createProject,
  appVersion,
}: {
  recentProjects: RecentProject[];
  openRecent: (project: RecentProject) => Promise<void>;
  removeRecent: (path: string) => void;
  openProject: () => Promise<void>;
  createProject: (name: string) => Promise<void>;
  appVersion: string;
}) {
  const [name, setName] = useState("Untitled");
  return (
    <main className="grid min-h-screen place-items-center bg-background p-4 text-foreground sm:p-8">
      <section className="grid w-full max-w-4xl overflow-hidden rounded-xl border bg-card shadow-2xl md:grid-cols-[1fr_1.25fr]">
        <div className="flex min-h-[440px] flex-col justify-between border-b bg-muted/30 p-8 md:border-b-0 md:border-r">
          <div><img className="mb-5 size-10" src={fpvEditorLogo} alt="FPV Editor" /><p className="font-heading text-xl font-semibold">FPV Editor</p><p className="mt-2 max-w-xs text-sm leading-6 text-muted-foreground">Start a new cut or pick up where your last flight left off.</p></div>
          <p className="font-mono text-[10px] uppercase tracking-[.16em] text-muted-foreground">Desktop cut suite · {appVersion ? `v${appVersion}` : "…"}</p>
        </div>
        <div className="p-8">
          <p className="mb-3 text-xs font-medium uppercase tracking-[.14em] text-muted-foreground">New project</p>
          <div className="flex gap-2"><Input value={name} onChange={(event) => setName(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void createProject(name); }} placeholder="Project name" /><Button onClick={() => void createProject(name)}><Plus data-icon="inline-start" />Create</Button></div>
          <Button className="mt-3 w-full" variant="outline" onClick={() => void openProject()}><FolderOpen data-icon="inline-start" />Open project file</Button>
          <Separator className="my-7" />
          <div className="mb-3 flex items-center justify-between"><p className="text-xs font-medium uppercase tracking-[.14em] text-muted-foreground">Recent projects</p><Badge variant="outline">{recentProjects.length}</Badge></div>
          {recentProjects.length ? <div className="space-y-1">{recentProjects.map((recent) => <div key={recent.path} className="flex items-center gap-1"><Button variant="ghost" className="h-auto min-w-0 flex-1 justify-start px-2 py-2" onClick={() => void openRecent(recent)}><Clapperboard data-icon="inline-start" /><span className="min-w-0 text-left"><span className="block truncate text-sm">{recent.name}</span><span className="block truncate font-mono text-[10px] text-muted-foreground">{recent.path}</span></span></Button><Button aria-label={`Remove ${recent.name} from recent projects`} variant="ghost" size="icon-sm" onClick={() => removeRecent(recent.path)}><Trash2 /></Button></div>)}</div> : <div className="rounded-lg border border-dashed p-5 text-center text-xs leading-5 text-muted-foreground">No recent projects yet. Create a project or open an existing <span className="font-mono">.fpv.json</span> file.</div>}
        </div>
      </section>
    </main>
  );
}

function App() {
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [preferences, setPreferences] = useState<EditorPreferences>(() => ({
    ...defaultPreferences,
    ...readLocal(preferencesKey, defaultPreferences),
  }));
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>(() => readLocal(recentProjectsKey, [] as RecentProject[]));
  const [project, setProject] = useState<Project>(seedProject);
  const [savedProject, setSavedProject] = useState(() => JSON.stringify(seedProject));
  const [selected, setSelected] = useState<string>();
  const [playhead, setPlayhead] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [previewMode, setPreviewMode] = useState<PreviewMode>("clip");
  const [previewRequestId, setPreviewRequestId] = useState(0);
  const [editTool, setEditTool] = useState<EditTool>("select");
  const [activePhase, setActivePhase] = useState<WorkflowPhase>("import");
  const [undoable, setUndoable] = useState(false);
  const [redoable, setRedoable] = useState(false);
  const [notice, setNotice] = useState("Ready to edit");
  const [dragActive, setDragActive] = useState(false);
  const { copilotOpen, mediaOpen, inspectorOpen, timelineHeight, timelineGrid } = preferences;
  const setPreference = <K extends keyof EditorPreferences>(key: K, value: EditorPreferences[K]) =>
    setPreferences((current) => ({ ...current, [key]: value }));
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [returnToOverviewOpen, setReturnToOverviewOpen] = useState(false);
  const [provider, setProvider] = useState(defaultProvider);
  const [appVersion, setAppVersion] = useState("");
  const [updateCheck, setUpdateCheck] = useState<UpdateCheckResult>();
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [downloadingUpdate, setDownloadingUpdate] = useState(false);
  const [mediaDiagnostics, setMediaDiagnostics] = useState<MediaDiagnostics>();
  const [checkingMediaDiagnostics, setCheckingMediaDiagnostics] = useState(false);
  const [prompt, setPrompt] = useState("");
  const [messages, setMessages] = useState([
    {
      role: "ai",
      text: "I can help make this flight cut tighter. Ask me to stabilize, trim, or assemble a sequence.",
    },
  ]);
  const selectedClip = selected ? project.clips[selected] : undefined;
  const activePhaseIndex = workflowPhases.findIndex((phase) => phase.id === activePhase);
  const activeWorkflowPhase = workflowPhases[activePhaseIndex];
  const showTimeline = activePhase === "cut";
  // The import phase is a dedicated media browser. Keeping the library out of
  // the narrow side rail gives source previews enough room to be useful.
  const showMedia = activePhase !== "export" && activePhase !== "import";
  const showInspector = activePhase === "stabilize" || activePhase === "cut" || activePhase === "grade";
  const showCopilot = activePhase === "cut" && copilotOpen;
  const hasUnsavedChanges = JSON.stringify(project) !== savedProject;
  const duration = useMemo(
    () =>
      Math.max(
        // Keep only a non-zero divisor for an empty project.  A one-second
        // floor made every sub-second trim look (and play) a full second long.
        1,
        ...Object.values(project.clips).map(
          (clip) => clip.position + clip.out_point - clip.in_point,
        ),
      ),
    [project],
  );
  const apply = (outcome: Outcome) => {
    setProject(outcome.project);
    setUndoable(outcome.can_undo);
    setRedoable(outcome.can_redo);
    const nextDuration = Math.max(
      1,
      ...Object.values(outcome.project.clips).map(
        (clip) => clip.position + clip.out_point - clip.in_point,
      ),
    );
    if (playhead > nextDuration) {
      setPlayhead(nextDuration);
      setPlaying(false);
    }
    // The monitor rendition is derived from the previous project state.
    // Requesting it again after every mutation makes trims visible at once.
    setPreviewRequestId((current) => current + 1);
  };
  useEffect(() => {
    // A tail trim can put the current playhead beyond the new project end.
    // Keep the ruler, monitor, and transport controls on the actual timeline.
    if (playhead > duration) {
      setPlayhead(duration);
      setPlaying(false);
    }
  }, [duration, playhead]);
  const command = async (value: object): Promise<Outcome | undefined> => {
    try {
      const outcome = await invoke<Outcome>("execute", { command: value });
      apply(outcome);
      return outcome;
    } catch (error) {
      setNotice(`Action failed: ${String(error)}`);
    }
  };
  const refresh = async () => {
    try {
      setProject(await invoke<Project>("timeline"));
    } catch {
      setNotice("Browser preview — Tauri IPC is unavailable here");
    }
  };
  useEffect(() => {
    void refresh();
    void getVersion().then(setAppVersion).catch(() => undefined);
  }, []);
  useEffect(() => {
    if (activePhase === "cut") setPreviewMode("timeline");
  }, [activePhase]);
  useEffect(() => { localStorage.setItem(preferencesKey, JSON.stringify(preferences)); }, [preferences]);
  const checkForUpdates = useCallback(async (silent = false) => {
    setCheckingUpdate(true);
    try {
      const result = await invoke<UpdateCheckResult>("check_for_updates");
      setUpdateCheck(result);
      if (!silent) {
        setNotice(
          result.update_available
            ? `Update available: v${result.latest_version}`
            : "You're up to date",
        );
      } else if (result.update_available) {
        setNotice(`Update available: v${result.latest_version} — see Settings`);
      }
    } catch (error) {
      if (!silent) setNotice(`Update check failed: ${String(error)}`);
    } finally {
      setCheckingUpdate(false);
    }
  }, []);
  const checkMediaDiagnostics = useCallback(async () => {
    setCheckingMediaDiagnostics(true);
    try {
      setMediaDiagnostics(await invoke<MediaDiagnostics>("media_diagnostics"));
    } catch (error) {
      setNotice(`FFmpeg check failed: ${String(error)}`);
    } finally {
      setCheckingMediaDiagnostics(false);
    }
  }, []);
  useEffect(() => {
    if (preferences.autoCheckUpdates) void checkForUpdates(true);
    // Only ever auto-check once per launch; the user can re-check manually afterwards.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const downloadUpdate = async () => {
    if (!updateCheck?.download_url || !updateCheck.asset_name) return;
    setDownloadingUpdate(true);
    try {
      setNotice(`Downloading ${updateCheck.asset_name}…`);
      await invoke("download_update", {
        downloadUrl: updateCheck.download_url,
        assetName: updateCheck.asset_name,
      });
      setNotice("Update downloaded — follow the installer to finish");
    } catch (error) {
      setNotice(`Update download failed: ${String(error)}`);
    } finally {
      setDownloadingUpdate(false);
    }
  };
  const rememberProject = (path: string, name: string) => {
    setRecentProjects((current) => {
      const next = [{ path, name, openedAt: Date.now() }, ...current.filter((item) => item.path !== path)].slice(0, 8);
      localStorage.setItem(recentProjectsKey, JSON.stringify(next));
      return next;
    });
  };
  const removeRecent = (path: string) => {
    setRecentProjects((current) => {
      const next = current.filter((item) => item.path !== path);
      localStorage.setItem(recentProjectsKey, JSON.stringify(next));
      return next;
    });
  };
  const addTrack = async (kind: TrackKind) =>
    command({
      command: "add_track",
      kind,
      name: `${kind === "Video" ? "V" : "A"}${project.tracks.filter((track) => track.kind === kind).length + 1}`,
    });
  const splitSelected = useCallback(async () => {
    const clip = selected ? project.clips[selected] : undefined;
    if (!clip) {
      setNotice("Select a clip to split");
      return;
    }
    const clipEnd = clip.position + clip.out_point - clip.in_point;
    if (playhead <= clip.position || playhead >= clipEnd) {
      setNotice("Move the playhead inside the selected clip to split it");
      return;
    }
    const outcome = await command({ command: "split_clip", clip_id: clip.id, at: playhead });
    if (outcome) setNotice("Clip split at playhead");
  }, [playhead, project.clips, selected]);
  const removeSelected = useCallback(async () => {
    if (!selected) {
      setNotice("Select a clip to remove");
      return;
    }
    const outcome = await command({ command: "remove_clip", clip_id: selected });
    if (outcome) {
      setSelected(undefined);
      setNotice("Clip removed");
    }
  }, [selected]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, [contenteditable='true']")) return;
      const modifier = event.metaKey || event.ctrlKey;
      if (modifier && event.key.toLowerCase() === "z") {
        event.preventDefault();
        void invoke<Outcome>(event.shiftKey ? "redo" : "undo").then(apply);
        return;
      }
      if (modifier && event.key.toLowerCase() === "y") {
        event.preventDefault();
        void invoke<Outcome>("redo").then(apply);
        return;
      }
      if (event.key.toLowerCase() === "v") setEditTool("select");
      if (event.key.toLowerCase() === "c") setEditTool((current) => current === "razor" ? "select" : "razor");
      if (event.key === "Delete" || event.key === "Backspace") {
        event.preventDefault();
        void removeSelected();
      }
      if (event.key === " ") {
        event.preventDefault();
        setPlaying((current) => !current);
      }
      if (event.key === "Enter") {
        event.preventDefault();
        void splitSelected();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [removeSelected, splitSelected]);
  const importPaths = useCallback(async (paths: string[], targetTrackId?: string) => {
    if (paths.length === 0) return;
    try {
      setNotice(`Reading ${paths.length === 1 ? fileName(paths[0]) : `${paths.length} sources`}…`);
      const outcome = await invoke<MediaImportOutcome>("import_media", { paths, targetTrackId });
      apply(outcome);
      const firstImported = Object.values(outcome.project.clips).find((clip) =>
        outcome.imported_paths.includes(clip.source_path),
      );
      if (firstImported) setSelected(firstImported.id);
      const skipped = outcome.skipped_paths.length;
      setNotice(
        `${outcome.imported_paths.length} linked ${outcome.imported_paths.length === 1 ? "clip" : "clips"}${skipped ? ` · ${skipped} skipped` : ""}`,
      );
    } catch (error) {
      setNotice(`Import failed: ${String(error)}`);
    }
  }, []);
  const importMedia = async () => {
    const selection = await open({
      multiple: true,
      filters: [{ name: "Video", extensions: ["mp4", "mov", "mkv", "m4v", "avi", "webm", "mts", "m2ts"] }],
    });
    if (!selection) return;
    await importPaths(Array.isArray(selection) ? selection : [selection]);
  };
  const importFolder = async () => {
    const selection = await open({ directory: true, multiple: false, recursive: true });
    if (typeof selection === "string") await importPaths([selection]);
  };
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    try {
      void getCurrentWindow()
        .onDragDropEvent((event) => {
          const payload = event.payload;
          if (payload.type === "enter") setDragActive(true);
          if (payload.type === "leave") setDragActive(false);
          if (payload.type === "drop") {
            setDragActive(false);
            void (async () => {
              const logicalPosition = payload.position.toLogical(await getCurrentWindow().scaleFactor());
              const targetTrackId = document
                .elementFromPoint(logicalPosition.x, logicalPosition.y)
                ?.closest<HTMLElement>("[data-track-id]")
                ?.dataset.trackId;
              await importPaths(payload.paths, targetTrackId);
            })();
          }
        })
        .then((stop) => {
          unlisten = stop;
        })
        .catch(() => undefined);
    } catch {
      // Browser preview does not expose Tauri's native drag-and-drop API.
    }
    return () => unlisten?.();
  }, [importPaths]);
  const saveProject = async () => {
    try {
      const path = await save({
        defaultPath: `${project.name || "flight"}.fpv.json`,
        filters: [{ name: "FPV Project", extensions: ["fpv.json"] }],
      });
      if (path) {
        await invoke("save_project", { path });
        rememberProject(path, project.name);
        setSavedProject(JSON.stringify(project));
        setNotice("Project saved");
        return true;
      }
    } catch (error) {
      setNotice(`Save failed: ${String(error)}`);
    }
    return false;
  };
  const loadProject = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "FPV Project", extensions: ["fpv.json", "json"] }],
      });
      if (typeof path === "string") {
        const loaded = await invoke<Project>("load_project", { path });
        setProject(loaded);
        setSavedProject(JSON.stringify(loaded));
        setSelected(undefined);
        rememberProject(path, loaded.name);
        setWorkspaceOpen(true);
        setNotice("Project loaded");
      }
    } catch (error) {
      setNotice(`Open failed: ${String(error)}`);
    }
  };
  const saveProvider = async (test = false) => {
    try {
      await invoke("configure_ai", { config: provider });
      if (test) {
        setNotice("Testing AI connection…");
        await invoke("test_ai_connection");
        setNotice("AI provider connected");
      } else {
        setNotice("AI settings saved for this session");
      }
    } catch (error) {
      setNotice(`AI connection failed: ${String(error)}`);
    }
  };
  const openRecent = async (recent: RecentProject) => {
    try {
      const loaded = await invoke<Project>("load_project", { path: recent.path });
      setProject(loaded); setSavedProject(JSON.stringify(loaded)); setSelected(undefined); rememberProject(recent.path, loaded.name); setWorkspaceOpen(true);
    } catch (error) { setNotice(`Open failed: ${String(error)}`); }
  };
  const createProject = async (name: string) => {
    try { const created = await invoke<Project>("new_project", { name }); setProject(created); setSavedProject(JSON.stringify(created)); setSelected(undefined); setWorkspaceOpen(true); } catch (error) { setNotice(`New project failed: ${String(error)}`); }
  };
  const returnToOverview = () => {
    setPlaying(false);
    setSelected(undefined);
    setWorkspaceOpen(false);
    setReturnToOverviewOpen(false);
  };
  if (!workspaceOpen) return <ProjectLauncher recentProjects={recentProjects} openRecent={openRecent} removeRecent={removeRecent} openProject={loadProject} createProject={createProject} appVersion={appVersion} />;
  const chat = async () => {
    if (!prompt.trim()) return;
    const text = prompt;
    setPrompt("");
    setMessages((items) => [...items, { role: "you", text }]);
    try {
      const response = await invoke<string>("chat", { prompt: text });
      setMessages((items) => [...items, { role: "ai", text: response }]);
      await refresh();
    } catch {
      setMessages((items) => [
        ...items,
        {
          role: "ai",
          text: "Configure an AI provider in Settings before using Copilot.",
        },
      ]);
    }
  };
  return (
    <TooltipProvider>
      <main className="relative min-h-screen min-w-[1120px] bg-background text-foreground">
        <header className="app-header border-b px-5">
          <div className="flex items-center gap-3">
            <img className="size-7" src={fpvEditorLogo} alt="" />
            <div>
              <p className="font-heading text-sm font-semibold">FPV Editor</p>
              <p className="text-[10px] uppercase tracking-[.16em] text-muted-foreground">
                Desktop cut suite
              </p>
            </div>
          </div>
          <nav className="header-workflow" aria-label="Editing workflow">
            <div className="header-workflow-phases" role="tablist" aria-label="Editing phases">
              {workflowPhases.map((phase, index) => {
                const Icon = phase.icon;
                const isActive = phase.id === activePhase;
                return <button key={phase.id} type="button" role="tab" aria-selected={isActive} title={phase.description} className={`workflow-phase ${isActive ? "is-active" : ""} ${index < activePhaseIndex ? "is-complete" : ""}`} onClick={() => setActivePhase(phase.id)}><span className="workflow-phase-number">{String(index + 1).padStart(2, "0")}</span><Icon /><span>{phase.label}</span></button>;
              })}
            </div>
          </nav>
          <div className="header-project">
            <Circle className="size-2 fill-foreground" />
            <span className="font-mono text-xs">{project.name}</span>
            <span className="text-xs text-muted-foreground">{project.width} × {project.height} · {project.fps} fps</span>
          </div>
          <div className="flex items-center justify-end gap-1">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => hasUnsavedChanges ? setReturnToOverviewOpen(true) : returnToOverview()}
            >
              <ChevronLeft data-icon="inline-start" />
              Projects
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void loadProject()}
            >
              <FolderOpen data-icon="inline-start" />
              Open
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void saveProject()}
            >
              <Download data-icon="inline-start" />
              Save
            </Button>
            <IconButton label="Settings" onClick={() => setSettingsOpen(true)} className="relative">
              <Settings />
              {updateCheck?.update_available && (
                <span className="absolute right-1 top-1 size-1.5 rounded-full bg-primary" />
              )}
            </IconButton>
          </div>
        </header>
        <div className="phase-toolbar border-b">
          <div><span className="font-mono text-[10px] uppercase tracking-[.14em] text-muted-foreground">{activeWorkflowPhase.eyebrow}</span><span className="phase-description">{activeWorkflowPhase.description}</span></div>
          <div className="workflow-actions">
            {activePhase === "import" && <><Button size="sm" onClick={() => void importMedia()}><Plus data-icon="inline-start" />Choose media</Button><Button variant="outline" size="sm" onClick={() => void importFolder()}><FolderInput data-icon="inline-start" />Choose folder</Button></>}
            {activePhase === "cut" && <Button variant={copilotOpen ? "secondary" : "ghost"} size="sm" onClick={() => setPreference("copilotOpen", !copilotOpen)}><Sparkles data-icon="inline-start" />Copilot</Button>}
            <IconButton label="Undo" disabled={!undoable} onClick={() => invoke<Outcome>("undo").then(apply)}><Undo2 /></IconButton><IconButton label="Redo" disabled={!redoable} onClick={() => invoke<Outcome>("redo").then(apply)}><RotateCw /></IconButton>
          </div>
        </div>
        <section
          className="grid min-h-0"
          style={{
            gridTemplateColumns: [
              ...(showMedia ? [mediaOpen ? "230px" : "42px"] : []),
              "minmax(420px,1fr)",
              ...(showInspector ? [inspectorOpen ? "270px" : "42px"] : []),
              ...(showCopilot ? ["310px"] : []),
            ].join(" "),
            height: `calc(100vh - ${showTimeline ? timelineHeight + 103 : 103}px)`,
          }}
        >
          {showMedia && <aside className="border-r bg-card">
            <PanelTitle icon={<Layers2 />} title="Media" side="left" open={mediaOpen} onToggle={() => setPreference("mediaOpen", !mediaOpen)} />
            {mediaOpen && <div className="p-3">
              {Object.values(project.clips).length === 0 ? (
                <EmptyMedia importMedia={importMedia} importFolder={importFolder} />
              ) : (
                <div className="flex flex-col gap-1">
                  {Object.values(project.clips).map((clip) => (
                    <Button
                      key={clip.id}
                      variant={selected === clip.id ? "secondary" : "ghost"}
                      className="h-auto justify-start px-2 py-2"
                      onClick={() => setSelected(clip.id)}
                    >
                      <Clapperboard data-icon="inline-start" />
                      <span className="min-w-0 text-left">
                        <span className="block truncate text-xs">
                          {fileName(clip.source_path)}
                        </span>
                        <span className="block font-mono text-[10px] text-muted-foreground">
                          {timecode(clip.out_point - clip.in_point)} · linked source
                        </span>
                      </span>
                    </Button>
                  ))}
                </div>
              )}
            </div>}
          </aside>}
          <section className="grid min-w-0 grid-rows-[minmax(0,1fr)_56px] bg-muted/30">
            {activePhase === "import" ? <ImportWorkspace
              clips={Object.values(project.clips)}
              selected={selected}
              selectClip={setSelected}
              importMedia={importMedia}
              importFolder={importFolder}
            /> : activePhase === "export" ? <ExportWorkspace project={project} selectedClip={selectedClip} saveProject={saveProject} /> : <Preview
              project={project}
              selectedClip={selectedClip}
              phase={activePhase}
              mode={previewMode}
              setMode={setPreviewMode}
              requestId={previewRequestId}
              playhead={playhead}
              duration={duration}
              playing={playing}
              setPlayhead={setPlayhead}
              setPlaying={setPlaying}
            />}
          </section>
          {showInspector && <aside className="border-l bg-card">
            <PanelTitle icon={<Command />} title="Inspector" side="right" open={inspectorOpen} onToggle={() => setPreference("inspectorOpen", !inspectorOpen)} />
            {inspectorOpen && (selectedClip ? (
              <ClipInspector clip={selectedClip} command={command} phase={activePhase} />
            ) : (
              <EmptyInspector />
            ))}
          </aside>}
          {showCopilot && (
            <Copilot
              messages={messages}
              prompt={prompt}
              setPrompt={setPrompt}
              chat={chat}
              close={() => setPreference("copilotOpen", false)}
            />
          )}
        </section>
        {showTimeline && <Timeline
          project={project}
          selected={selected}
          duration={duration}
          playhead={playhead}
          setPlayhead={setPlayhead}
          onPreviewTimeline={() => {
            setPreviewMode("timeline");
            setPreviewRequestId((current) => current + 1);
          }}
          tool={editTool}
          setTool={setEditTool}
          onSelect={setSelected}
          onSplit={(clipId, at) => command({ command: "split_clip", clip_id: clipId, at })}
          onRemove={removeSelected}
          onMove={(clipId, trackId, position) => command({ command: "move_clip", clip_id: clipId, new_track_id: trackId, new_position: position })}
          onTrimEnd={(clip, newOut) => command({ command: "trim_clip", clip_id: clip.id, new_in: clip.in_point, new_out: newOut })}
          onTrimStart={(clip, newIn, newPosition) => command({ command: "trim_clip_start", clip_id: clip.id, new_in: newIn, new_position: newPosition })}
          onAddTrack={addTrack}
          height={timelineHeight}
          setHeight={(height) => setPreference("timelineHeight", height)}
          grid={timelineGrid}
        />}
        <SettingsDialog
          open={settingsOpen}
          setOpen={setSettingsOpen}
          provider={provider}
          project={project}
          notice={notice}
          setProvider={setProvider}
          saveProvider={saveProvider}
          appVersion={appVersion}
          autoCheckUpdates={preferences.autoCheckUpdates}
          setAutoCheckUpdates={(value) => setPreference("autoCheckUpdates", value)}
          timelineGrid={timelineGrid}
          setTimelineGrid={(value) => setPreference("timelineGrid", value)}
          updateCheck={updateCheck}
          checkingUpdate={checkingUpdate}
          downloadingUpdate={downloadingUpdate}
          checkForUpdates={() => checkForUpdates(false)}
          downloadUpdate={downloadUpdate}
          mediaDiagnostics={mediaDiagnostics}
          checkingMediaDiagnostics={checkingMediaDiagnostics}
          checkMediaDiagnostics={checkMediaDiagnostics}
        />
        <Dialog open={returnToOverviewOpen} onOpenChange={setReturnToOverviewOpen}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Unsaved changes</DialogTitle>
              <DialogDescription>
                Save your changes before returning to the project overview, or discard them.
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="outline" onClick={() => setReturnToOverviewOpen(false)}>Cancel</Button>
              <Button variant="destructive" onClick={returnToOverview}>Discard changes</Button>
              <Button onClick={() => void saveProject().then((saved) => { if (saved) returnToOverview(); })}>Save and return</Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
        {dragActive && (
          <div className="pointer-events-none absolute inset-3 grid place-items-center rounded-xl border-2 border-dashed bg-background/90">
            <div className="flex flex-col items-center gap-3 text-center">
              <Upload className="text-muted-foreground" />
              <div>
                <p className="text-sm font-medium">Drop media or a folder to link it</p>
                <p className="mt-1 text-xs text-muted-foreground">Original files stay where they are, including network storage.</p>
              </div>
            </div>
          </div>
        )}
      </main>
    </TooltipProvider>
  );
}
function PanelTitle({
  icon,
  title,
  side,
  open,
  onToggle,
}: {
  icon: React.ReactNode;
  title: string;
  side: "left" | "right";
  open?: boolean;
  onToggle?: () => void;
}) {
  const isOpen = open !== false;
  const ToggleIcon = side === "left"
    ? (isOpen ? ChevronsLeft : ChevronsRight)
    : (isOpen ? ChevronsRight : ChevronsLeft);
  return (
    <div className={`flex h-11 items-center border-b text-xs font-medium ${isOpen ? "gap-2 px-2" : "justify-center px-0"}`}>
      {isOpen && <span className="text-muted-foreground">{icon}</span>}
      {isOpen && <span className="flex-1">{title}</span>}
      {onToggle && (
        <IconButton label={`${isOpen ? "Collapse" : "Expand"} ${title}`} onClick={onToggle}>
          <ToggleIcon />
        </IconButton>
      )}
    </div>
  );
}
function EmptyMedia({
  importMedia,
  importFolder,
}: {
  importMedia: () => Promise<void>;
  importFolder: () => Promise<void>;
}) {
  return (
    <Card size="sm" className="border-dashed bg-transparent shadow-none">
      <CardHeader>
        <CardTitle className="text-sm">No media yet</CardTitle>
        <CardDescription className="text-xs">
          Link files, folders, or drop them here. Originals are never copied.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <Button size="sm" onClick={() => void importMedia()}>
          <Plus data-icon="inline-start" />
          Choose media
        </Button>
        <Button variant="outline" size="sm" onClick={() => void importFolder()}>
          <FolderInput data-icon="inline-start" />
          Choose folder
        </Button>
      </CardContent>
    </Card>
  );
}
function ImportWorkspace({
  clips,
  selected,
  selectClip,
  importMedia,
  importFolder,
}: {
  clips: Clip[];
  selected?: string;
  selectClip: (id: string) => void;
  importMedia: () => Promise<void>;
  importFolder: () => Promise<void>;
}) {
  return (
    <section className="import-workspace overflow-y-auto">
      <div className="import-workspace-heading">
        <div>
          <p className="import-kicker">Source library</p>
          <h1>Build your flight bin.</h1>
          <p className="import-subtitle">Link original footage without moving it. Every source remains exactly where you shot it.</p>
        </div>
        <div className="import-count" aria-label={`${clips.length} imported clips`}><span>{String(clips.length).padStart(2, "0")}</span> clips linked</div>
      </div>
      {clips.length === 0 ? (
        <div className="import-empty">
          <div className="import-empty-icon"><Video /></div>
          <p className="import-kicker">Nothing in the bin</p>
          <h2>Bring in the footage.</h2>
          <p>Choose individual video files, an entire card dump, or drag sources into this window.</p>
          <div className="import-empty-actions">
            <Button onClick={() => void importMedia()}><Plus data-icon="inline-start" />Choose media</Button>
            <Button variant="outline" onClick={() => void importFolder()}><FolderInput data-icon="inline-start" />Choose folder</Button>
          </div>
          <span className="import-empty-note">MP4 · MOV · MKV · M4V · AVI · WebM · MTS</span>
        </div>
      ) : (
        <div className="import-gallery" aria-label="Imported media">
          {clips.map((clip, index) => <ImportMediaCard key={clip.id} clip={clip} index={index} selected={selected === clip.id} selectClip={selectClip} />)}
          <button type="button" className="import-add-card" onClick={() => void importMedia()}>
            <Plus />
            <span>Add more media</span>
          </button>
        </div>
      )}
    </section>
  );
}
function ImportMediaCard({ clip, index, selected, selectClip }: { clip: Clip; index: number; selected: boolean; selectClip: (id: string) => void }) {
  const [unavailable, setUnavailable] = useState(false);
  const source = useMemo(() => mediaSource(clip.source_path), [clip.source_path]);
  const duration = timecode(clip.out_point - clip.in_point);
  return (
    <button type="button" className={`import-media-card ${selected ? "is-selected" : ""}`} onClick={() => selectClip(clip.id)} aria-pressed={selected}>
      <div className="import-thumbnail">
        {source && !unavailable ? (
          <video src={source} muted playsInline preload="metadata" onError={() => setUnavailable(true)} onLoadedMetadata={(event) => { event.currentTarget.currentTime = 0.04; }} />
        ) : (
          <div className="import-thumbnail-fallback"><Clapperboard /><span>Preview unavailable</span></div>
        )}
        <span className="import-thumbnail-index">{String(index + 1).padStart(2, "0")}</span>
        <span className="import-thumbnail-duration">{duration}</span>
        {selected && <span className="import-selected-mark">Selected</span>}
      </div>
      <span className="import-card-copy">
        <span className="import-card-name">{fileName(clip.source_path)}</span>
        <span className="import-card-source">Linked original · {duration}</span>
      </span>
    </button>
  );
}
function EmptyInspector() {
  return (
    <div className="grid h-[calc(100%-44px)] place-items-center p-6 text-center">
      <div>
        <Circle className="mx-auto mb-3 size-7 text-muted-foreground" />
        <p className="text-sm font-medium">Nothing selected</p>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">
          Select a clip to adjust its stabilization and speed.
        </p>
      </div>
    </div>
  );
}
function ExportWorkspace({ project, selectedClip, saveProject }: { project: Project; selectedClip?: Clip; saveProject: () => Promise<boolean> }) {
  return <section className="grid place-items-center p-8"><div className="w-full max-w-xl"><Badge variant="outline" className="mb-5 font-mono text-[10px] uppercase tracking-[.18em]">Final check</Badge><h1 className="font-heading text-2xl font-semibold">Ready to deliver?</h1><p className="mt-2 max-w-md text-sm leading-6 text-muted-foreground">Review the project settings, save your edit, then render from the export pipeline.</p><div className="mt-7 grid grid-cols-3 gap-px overflow-hidden rounded-lg border bg-border"><div className="bg-card p-4"><p className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">Clips</p><p className="mt-1 text-lg font-medium">{Object.keys(project.clips).length}</p></div><div className="bg-card p-4"><p className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">Format</p><p className="mt-1 text-lg font-medium">{project.width}×{project.height}</p></div><div className="bg-card p-4"><p className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">Frame rate</p><p className="mt-1 text-lg font-medium">{project.fps} fps</p></div></div><Card className="mt-5 shadow-none"><CardHeader><CardTitle className="text-sm">Desktop export</CardTitle><CardDescription className="text-xs">Save the project now. Final rendering is currently available through the project export pipeline; this workspace keeps delivery settings separate from the edit.</CardDescription></CardHeader><CardContent><Button onClick={() => void saveProject()}><Download data-icon="inline-start" />Save project</Button>{selectedClip && <p className="mt-3 text-xs text-muted-foreground">Selected clip: {fileName(selectedClip.source_path)}</p>}</CardContent></Card></div></section>;
}
function Preview({
  project,
  selectedClip,
  phase,
  mode,
  setMode,
  requestId,
  playhead,
  duration,
  playing,
  setPlayhead,
  setPlaying,
}: {
  project: Project;
  selectedClip?: Clip;
  phase: WorkflowPhase;
  mode: PreviewMode;
  setMode: (mode: PreviewMode) => void;
  requestId: number;
  playhead: number;
  duration: number;
  playing: boolean;
  setPlayhead: (value: number) => void;
  setPlaying: (value: boolean) => void;
}) {
  const video = useRef<HTMLVideoElement>(null);
  const [source, setSource] = useState<string>();
  const [rendering, setRendering] = useState(false);
  const [error, setError] = useState<string>();
  const [previewStart, setPreviewStart] = useState(0);
  const [bufferedEnd, setBufferedEnd] = useState(0);
  const [mediaDuration, setMediaDuration] = useState(0);
  const [timelineRenderVersion, setTimelineRenderVersion] = useState(0);
  const requestedTimelineStart = useRef<number | undefined>(undefined);
  const playheadUpdatedByPlayback = useRef(false);
  const selectedId = selectedClip?.id;
  useEffect(() => {
    let active = true;
    // Imports should be instant: the original is already playable and we
    // defer effect-aware rendering until the user enters an edit phase.
    setSource(mode === "clip" && selectedClip ? mediaSource(selectedClip.source_path) : undefined);
    if (mode === "clip") {
      setPreviewStart(0);
      setBufferedEnd(0);
      setMediaDuration(0);
    }
    setError(undefined);
    if (mode === "clip" && !selectedId) return;
    if (mode === "clip" && phase === "import") return;
    // Tauri deserializes Timecode as an i64 microsecond value. Browser media
    // events provide fractional milliseconds, so normalize before IPC.
    const start = mode === "timeline"
      ? requestedTimelineStart.current ?? Math.round(playhead)
      : 0;
    requestedTimelineStart.current = undefined;
    setPreviewStart(start);
    setBufferedEnd(start);
    setMediaDuration(0);
    setRendering(true);
    void invoke<string>("render_preview", {
      clipId: mode === "clip" ? selectedId : null,
      start: mode === "timeline" ? start : null,
    })
      .then((path) => {
        if (active) setSource(mediaSource(path));
      })
      .catch((reason) => {
        if (!active) return;
        // Development in a browser has no Tauri renderer; direct source
        // playback still makes the single-clip monitor useful there.
        setError(`Preview render failed: ${String(reason)}`);
      })
      .finally(() => {
        if (active) setRendering(false);
      });
    return () => { active = false; };
  }, [mode, phase, requestId, selectedId, selectedClip, timelineRenderVersion]);
  useEffect(() => {
    const element = video.current;
    if (!element) return;
    if (playing) void element.play().catch(() => setPlaying(false));
    else element.pause();
  }, [playing, source, setPlaying]);
  useEffect(() => {
    const element = video.current;
    if (!element || !Number.isFinite(element.duration)) return;
    const target = (playhead - previewStart) / 1_000_000;
    // A timeline preview is a bounded window. Never clamp an out-of-window
    // seek to this video's start/end: its timeupdate would overwrite the
    // requested timeline position with that stale boundary.
    if (target < 0 || target > element.duration) return;
    // Native playback updates the shared playhead too. Only seek when the
    // difference is meaningful, otherwise normal playback would stutter.
    if (Math.abs(element.currentTime - target) > 0.04) element.currentTime = target;
  }, [playhead, source]);
  useEffect(() => {
    if (mode !== "timeline" || !source || mediaDuration <= 0) return;
    if (playheadUpdatedByPlayback.current) {
      playheadUpdatedByPlayback.current = false;
      return;
    }
    const previewEnd = previewStart + mediaDuration * 1_000_000;
    if (playhead < previewStart || playhead > previewEnd) {
      // Seeking beyond the currently rendered range requires a new monitor
      // window. Pause until it is ready so the obsolete source cannot move
      // the shared playhead back to its own boundary.
      requestedTimelineStart.current = Math.round(playhead);
      setPlaying(false);
      setTimelineRenderVersion((current) => current + 1);
    }
  }, [mediaDuration, mode, playhead, previewStart, setPlaying, source]);
  const seek = (next: number) => {
    const value = Math.max(0, Math.min(duration, next));
    setPlayhead(value);
  };
  const seekToPlayhead = (element: HTMLVideoElement) => {
    element.currentTime = Math.min(element.duration || 0, (playhead - previewStart) / 1_000_000);
  };
  const updateBufferedRange = (element: HTMLVideoElement) => {
    const ranges = element.buffered;
    if (!ranges.length) return;
    setBufferedEnd(previewStart + ranges.end(ranges.length - 1) * 1_000_000);
  };
  const activeClip = mode === "timeline"
    ? Object.values(project.clips).find((clip) => playhead >= clip.position && playhead < clip.position + clip.out_point - clip.in_point) ?? selectedClip
    : selectedClip;
  const clipTime = activeClip
    ? Math.max(0, playhead - activeClip.position + activeClip.in_point)
    : 0;
  // The bar intentionally represents the active preview window, not the full
  // project. A 12-second buffer must remain legible in a 20-minute timeline.
  const windowEnd = mode === "timeline"
    ? Math.min(duration, previewStart + 12_000_000)
    : previewStart + mediaDuration * 1_000_000;
  const windowSpan = Math.max(1, windowEnd - previewStart);
  const bufferProgress = Math.max(0, Math.min(100, ((bufferedEnd - previewStart) / windowSpan) * 100));
  const playheadInWindow = Math.max(0, Math.min(100, ((playhead - previewStart) / windowSpan) * 100));
  const bufferedSeconds = Math.max(0, (bufferedEnd - previewStart) / 1_000_000);
  return (
    <section className="grid min-w-0 grid-rows-[minmax(0,1fr)_56px]">
      <div className="grid place-items-center p-6">
        <div className="relative aspect-video w-[min(76vh,76%)] overflow-hidden rounded-lg border bg-card shadow-2xl">
          <div className="absolute inset-0 preview-grid" />
          <div className="absolute inset-[8%] border border-dashed border-border/70" />
          <div className="absolute left-[-10%] top-1/2 w-[120%] border-t border-foreground/30 -rotate-6" />
          {source ? (
            <video
              ref={video}
              key={source}
              className="absolute inset-0 size-full bg-black object-contain"
              preload="auto"
              src={source}
              onLoadedMetadata={(event) => {
                setMediaDuration(event.currentTarget.duration || 0);
                seekToPlayhead(event.currentTarget);
                updateBufferedRange(event.currentTarget);
              }}
              onCanPlay={(event) => updateBufferedRange(event.currentTarget)}
              onProgress={(event) => updateBufferedRange(event.currentTarget)}
              onTimeUpdate={(event) => {
                updateBufferedRange(event.currentTarget);
                playheadUpdatedByPlayback.current = !event.currentTarget.paused;
                setPlayhead(previewStart + event.currentTarget.currentTime * 1_000_000);
              }}
              onPlay={() => setPlaying(true)}
              onPause={() => setPlaying(false)}
              onEnded={() => setPlaying(false)}
            >
              Your system cannot play this source format.
            </video>
          ) : (
            <div className="absolute inset-0 grid place-items-center text-center">
              <div>
                <Badge
                  variant="outline"
                  className="mb-3 font-mono text-[10px] uppercase tracking-[.18em]"
                >
                  Monitor 01
                </Badge>
                <h1 className="font-heading text-base font-semibold">
                  Select a clip to preview
                </h1>
                <p className="mt-2 font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
                  Link a source to start editing
                </p>
              </div>
            </div>
          )}
          {rendering && (
            <div
              className="absolute inset-x-5 top-5 z-10 flex items-center gap-3 rounded-md border border-amber-300/30 bg-black/80 px-4 py-3 shadow-xl backdrop-blur-sm"
              role="status"
              aria-live="polite"
            >
              <RefreshCw className="size-4 shrink-0 animate-spin text-amber-300" />
              <div className="min-w-0">
                <p className="font-mono text-[10px] font-semibold uppercase tracking-[.14em] text-amber-200">
                  {mode === "timeline" ? "Rendering timeline preview" : "Rendering clip preview"}
                </p>
                <p className="mt-0.5 text-xs text-white/65">
                  Preparing edits and effects. You can continue editing while this runs.
                </p>
              </div>
            </div>
          )}
          <div className="absolute bottom-3 left-4 right-4 space-y-1.5 font-mono text-[10px] text-muted-foreground">
            <div className="flex justify-between">
              <span>{timecode(playhead)}</span>
              <span>{rendering ? "Preparing buffer" : mode === "timeline" ? "Timeline preview" : "Clip preview"}</span>
            </div>
            {source && (
              <>
                <div className="relative h-1.5 overflow-hidden rounded-full border border-white/10 bg-black/45" aria-label={`${bufferedSeconds.toFixed(1)} seconds buffered`}>
                  <div className="absolute inset-y-0 left-0 bg-emerald-400/85 transition-[width] duration-150" style={{ width: `${bufferProgress}%` }} />
                  <div className="absolute -top-0.5 size-2.5 rounded-full border border-black/40 bg-white shadow" style={{ left: `calc(${playheadInWindow}% - 5px)` }} />
                </div>
                <div className="flex justify-between text-[9px] uppercase tracking-wider text-white/55">
                  <span>{activeClip ? `${fileName(activeClip.source_path)} · source ${timecode(clipTime)}` : "Timeline"}</span>
                  <span>{bufferedSeconds.toFixed(1)}s buffered · → {timecode(bufferedEnd)}</span>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
      <div className="flex items-center justify-center gap-7 border-t">
        <span className="font-mono text-xs text-muted-foreground">
          {timecode(playhead)}
        </span>
        <div className="flex items-center gap-1">
          <IconButton
            label="Previous second"
            onClick={() => seek(playhead - 1_000_000)}
          >
            <ChevronLeft />
          </IconButton>
          <Button
            aria-label="Play or pause"
            size="icon"
            className="rounded-full"
            onClick={() => setPlaying(!playing)}
          >
            {playing ? <Pause /> : <Play />}
          </Button>
          <IconButton
            label="Next second"
            onClick={() => seek(playhead + 1_000_000)}
          >
            <ChevronRight />
          </IconButton>
        </div>
        <span className="font-mono text-xs text-muted-foreground">
          {timecode(duration)}
        </span>
        <Button
          size="xs"
          variant="outline"
          onClick={() => setMode(mode === "timeline" ? "clip" : "timeline")}
        >
          {mode === "timeline" ? "Show clip" : "Show timeline"}
        </Button>
      </div>
      {error && <p className="px-3 pb-1 text-center text-[10px] text-destructive">{error}</p>}
    </section>
  );
}
function ClipInspector({
  clip,
  command,
  phase,
}: {
  clip: Clip;
  command: (value: object) => Promise<Outcome | undefined>;
  phase: WorkflowPhase;
}) {
  const [smoothness, setSmoothness] = useState(
    clip.stabilization?.smoothness ?? 0.55,
  );
  const profile = (more: Partial<Profile> = {}) => ({
    smoothness,
    strength: clip.stabilization?.strength ?? 1,
    horizon_lock: clip.stabilization?.horizon_lock ?? true,
    dynamic_fov: clip.stabilization?.dynamic_fov ?? 0.12,
    ...more,
  });
  const chooseLut = async () => {
    const selection = await open({ multiple: false, filters: [{ name: "LUT", extensions: ["cube", "3dl"] }] });
    if (typeof selection === "string") void command({ command: "apply_lut", clip_id: clip.id, lut_path: selection });
  };
  return (
    <div className="h-[calc(100%-44px)]">
      <div className="flex flex-col gap-5 p-4">
        <Card size="sm" className="shadow-none">
          <CardHeader>
            <CardTitle className="truncate text-xs">
              {fileName(clip.source_path)}
            </CardTitle>
            <CardDescription className="font-mono text-[10px]">
              {timecode(clip.out_point - clip.in_point)} · SOURCE
            </CardDescription>
          </CardHeader>
        </Card>
        {phase === "stabilize" && <div className="flex flex-col gap-3">
          <div className="flex items-center justify-between">
            <p className="text-xs font-medium">Stabilization</p>
            <Badge variant={clip.stabilization ? "secondary" : "outline"}>
              {clip.stabilization ? "Applied" : "Not applied"}
            </Badge>
          </div>
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>Smoothness</span>
            <span className="font-mono">{Math.round(smoothness * 100)}%</span>
          </div>
          <Slider
            value={[smoothness]}
            max={1}
            step={0.05}
            onValueChange={([value]) => setSmoothness(value)}
            onValueCommit={() =>
              void command({
                command: "apply_stabilization",
                clip_id: clip.id,
                profile: profile(),
              })
            }
          />
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              void command({
                command: "apply_stabilization",
                clip_id: clip.id,
                profile: profile(),
              })
            }
          >
            Apply stabilization
          </Button>
          <div className="flex items-center justify-between">
            <span className="text-xs text-muted-foreground">Horizon lock</span>
            <Switch
              checked={profile().horizon_lock}
              onCheckedChange={(horizon_lock) =>
                void command({
                  command: "apply_stabilization",
                  clip_id: clip.id,
                  profile: profile({ horizon_lock }),
                })
              }
            />
          </div>
        </div>}
        {phase === "cut" && <>
        <ToggleGroup
          type="single"
          defaultValue="1"
          variant="outline"
          spacing={0}
          className="w-full"
          onValueChange={(rate) => {
            if (rate)
              void command({
                command: "set_speed_ramp",
                clip_id: clip.id,
                keyframes: [{ at: 0, rate: Number(rate) }],
              });
          }}
        >
          <ToggleGroupItem value=".5" className="flex-1 text-xs">
            0.5×
          </ToggleGroupItem>
          <ToggleGroupItem value="1" className="flex-1 text-xs">
            1×
          </ToggleGroupItem>
          <ToggleGroupItem value="2" className="flex-1 text-xs">
            2×
          </ToggleGroupItem>
        </ToggleGroup>
        <Button
          variant="outline"
          size="sm"
          onClick={() =>
            void command({ command: "remove_clip", clip_id: clip.id })
          }
        >
          <Trash2 data-icon="inline-start" />
          Remove clip
        </Button>
        </>}
        {phase === "grade" && <div className="flex flex-col gap-3">
          <div className="flex items-center justify-between"><p className="text-xs font-medium">Color treatment</p><Badge variant={clip.lut_path ? "secondary" : "outline"}>{clip.lut_path ? "Applied" : "No LUT"}</Badge></div>
          <Card size="sm" className="shadow-none"><CardHeader><CardTitle className="truncate text-xs">{clip.lut_path ? fileName(clip.lut_path) : "No LUT selected"}</CardTitle><CardDescription className="text-xs">LUTs are applied during preview and export.</CardDescription></CardHeader></Card>
          <Button size="sm" onClick={() => void chooseLut()}><Film data-icon="inline-start" />Choose LUT</Button>
        </div>}
      </div>
    </div>
  );
}
function Copilot({
  messages,
  prompt,
  setPrompt,
  chat,
  close,
}: {
  messages: { role: string; text: string }[];
  prompt: string;
  setPrompt: (value: string) => void;
  chat: () => Promise<void>;
  close: () => void;
}) {
  return (
    <aside className="grid min-h-0 grid-rows-[44px_1fr_auto_auto] border-l bg-card">
      <div className="flex h-11 items-center justify-between border-b px-3 text-xs font-medium">
        <span className="flex items-center gap-2">
          <WandSparkles />
          Copilot
        </span>
        <IconButton label="Close Copilot" onClick={close}>
          <X />
        </IconButton>
      </div>
      <div className="flex min-h-0 flex-col gap-3 overflow-auto p-4">
        {messages.map((message, index) => (
          <div
            key={index}
            className={
              message.role === "you"
                ? "ml-7 rounded-lg bg-primary px-3 py-2 text-xs leading-5 text-primary-foreground"
                : "mr-4 rounded-lg border bg-muted/50 px-3 py-2 text-xs leading-5"
            }
          >
            {message.text}
          </div>
        ))}
      </div>
      <div className="flex flex-wrap gap-1 px-3 pb-2">
        <Button
          variant="outline"
          size="xs"
          onClick={() =>
            setPrompt("Stabilize the selected clip with horizon lock")
          }
        >
          Stabilize this take
        </Button>
        <Button
          variant="outline"
          size="xs"
          onClick={() =>
            setPrompt("Find the best moments for a fast freestyle cut")
          }
        >
          Find best moments
        </Button>
      </div>
      <div className="border-t p-3">
        <div className="flex gap-2">
          <Textarea
            value={prompt}
            onChange={(event) => setPrompt(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                void chat();
              }
            }}
            className="min-h-10 resize-none text-xs"
            placeholder="Ask Copilot to shape the edit…"
          />
          <Button
            aria-label="Send to Copilot"
            size="icon"
            onClick={() => void chat()}
          >
            <Send />
          </Button>
        </div>
      </div>
    </aside>
  );
}
function Timeline({
  project,
  selected,
  duration,
  playhead,
  setPlayhead,
  onPreviewTimeline,
  tool,
  setTool,
  onSelect,
  onSplit,
  onRemove,
  onMove,
  onTrimStart,
  onTrimEnd,
  onAddTrack,
  height,
  setHeight,
  grid,
}: {
  project: Project;
  selected?: string;
  duration: number;
  playhead: number;
  setPlayhead: (value: number) => void;
  onPreviewTimeline: () => void;
  tool: EditTool;
  setTool: (tool: EditTool) => void;
  onSelect: (id: string) => void;
  onSplit: (clipId: string, at: number) => Promise<Outcome | undefined>;
  onRemove: () => Promise<void>;
  onMove: (clipId: string, trackId: string, position: number) => Promise<Outcome | undefined>;
  onTrimStart: (clip: Clip, newIn: number, newPosition: number) => Promise<Outcome | undefined>;
  onTrimEnd: (clip: Clip, newOut: number) => Promise<Outcome | undefined>;
  onAddTrack: (kind: TrackKind) => Promise<Outcome | undefined>;
  height: number;
  setHeight: (height: number) => void;
  grid: TimelineGrid;
}) {
  const [draggingClip, setDraggingClip] = useState<{ id: string; position: number; trackId: string }>();
  const [razorPosition, setRazorPosition] = useState<number>();
  const rulerTicks = useMemo(() => {
    const totalSeconds = Math.max(0.001, duration / 1_000_000);
    const targetStep = totalSeconds / 6;
    const step = [0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600]
      .find((candidate) => candidate >= targetStep) ?? 7200;
    const ticks = Array.from(
      { length: Math.floor(totalSeconds / step) + 1 },
      (_, index) => Math.round(index * step * 1_000_000),
    );
    // Always label the actual timeline end, even when it is not on a neat
    // interval. This keeps the ruler, playhead, and rendered duration honest.
    if (ticks[ticks.length - 1] !== duration) ticks.push(duration);
    return ticks;
  }, [duration]);
  const toTimecode = (value: number) => Math.round(Math.max(0, value));
  const timeAtPointer = (event: { clientX: number }, lane: HTMLElement) =>
    toTimecode(Math.min(duration, ((event.clientX - lane.getBoundingClientRect().left) / lane.getBoundingClientRect().width) * duration));
  const splitClipAt = (clip: Clip, at: number) => {
    const edgePadding = 1_000;
    const start = clip.position + edgePadding;
    const end = clip.position + clip.out_point - clip.in_point - edgePadding;
    if (end <= start) return;
    void onSplit(clip.id, toTimecode(Math.max(start, Math.min(end, at))));
  };
  const resize = (event: React.PointerEvent<HTMLButtonElement>) => {
    const startY = event.clientY;
    const startHeight = height;
    const move = (moveEvent: PointerEvent) =>
      setHeight(Math.min(520, Math.max(140, startHeight + startY - moveEvent.clientY)));
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop, { once: true });
  };
  const movePlayhead = (event: React.PointerEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    const handle = event.currentTarget;
    const rail = handle.parentElement;
    if (!rail) return;
    const update = (pointerEvent: { clientX: number }) => setPlayhead(timeAtPointer(pointerEvent, rail));
    update(event);
    handle.setPointerCapture(event.pointerId);
    const move = (pointerEvent: PointerEvent) => update(pointerEvent);
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop, { once: true });
  };
  const clipMoveAt = (pointerEvent: { clientX: number; clientY: number }, clip: Clip, originX: number) => {
    const target = document.elementFromPoint(pointerEvent.clientX, pointerEvent.clientY)?.closest<HTMLElement>("[data-track-id]");
    const lane = target?.querySelector<HTMLElement>("[data-timeline-lane]");
    const trackId = target?.dataset.trackId;
    if (!lane || !trackId) return undefined;
    const offset = ((pointerEvent.clientX - originX) / lane.getBoundingClientRect().width) * duration;
    return { position: toTimecode(Math.max(0, clip.position + offset)), trackId };
  };
  const moveClip = (event: React.PointerEvent<HTMLButtonElement>, clip: Clip) => {
    if (tool !== "select" || event.button !== 0) return;
    const handle = event.currentTarget;
    const originX = event.clientX;
    const sourceTrack = handle.closest<HTMLElement>("[data-track-id]");
    if (!sourceTrack) return;
    handle.setPointerCapture(event.pointerId);
    let moved = false;
    const move = (moveEvent: PointerEvent) => {
      const next = clipMoveAt(moveEvent, clip, originX);
      if (!next) return;
      moved ||= Math.abs(moveEvent.clientX - originX) >= 3;
      if (moved) setDraggingClip({ id: clip.id, ...next });
    };
    const finish = (upEvent: PointerEvent) => {
      const next = clipMoveAt(upEvent, clip, originX);
      handle.removeEventListener("pointermove", move);
      setDraggingClip(undefined);
      if (moved && next) void onMove(clip.id, next.trackId, next.position);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", finish, { once: true });
    // A click still selects; the pointer-up handler above only mutates after a drag.
    onSelect(clip.id);
  };
  const trimClip = (event: React.PointerEvent<HTMLElement>, clip: Clip, edge: "start" | "end") => {
    event.stopPropagation();
    const lane = event.currentTarget.closest<HTMLElement>("[data-timeline-lane]");
    if (!lane) return;
    const startX = event.clientX;
    const startIn = clip.in_point;
    const startOut = clip.out_point;
    const startPosition = clip.position;
    event.currentTarget.setPointerCapture(event.pointerId);
    const finish = (upEvent: PointerEvent) => {
      const delta = ((upEvent.clientX - startX) / lane.getBoundingClientRect().width) * duration;
      const minimumDuration = 1_000;
      if (edge === "start") {
        const deltaClamped = Math.max(-startPosition, -startIn, Math.min(delta, startOut - startIn - minimumDuration));
        void onTrimStart(clip, toTimecode(startIn + deltaClamped), toTimecode(startPosition + deltaClamped));
      } else {
        void onTrimEnd(clip, toTimecode(Math.max(startIn + minimumDuration, startOut + delta)));
      }
    };
    event.currentTarget.addEventListener("pointerup", finish, { once: true });
  };
  return (
    <section
      className={`timeline-editor timeline-grid-${grid} relative overflow-hidden border-t bg-card ${tool === "razor" ? "is-razor" : "is-select"}`}
      style={{ height }}
      onPointerLeave={() => setRazorPosition(undefined)}
    >
      <button aria-label="Resize timeline" className="timeline-resize-handle" onPointerDown={resize}>
        <GripHorizontal />
      </button>
      <div className="grid h-12 grid-cols-[148px_minmax(0,1fr)] border-b">
        <div className="flex items-center gap-2 border-r px-4">
          <span className="text-xs font-medium">Timeline</span>
          <Badge variant="outline" className="font-mono text-[9px]">
            MAGNETIC
          </Badge>
          {tool === "razor" && <span className="font-mono text-[9px] uppercase tracking-wide text-foreground">Razor active</span>}
        </div>
        <div className="timeline-tools flex items-center justify-end gap-1 px-2">
          <Button
            aria-pressed={tool === "select"}
            className={`timeline-tool ${tool === "select" ? "is-active" : ""}`}
            size="xs"
            variant="ghost"
            onClick={() => setTool("select")}
          >
            <MousePointer2 data-icon="inline-start" />
            Select <kbd>V</kbd>
          </Button>
          <Button
            aria-pressed={tool === "razor"}
            className={`timeline-tool ${tool === "razor" ? "is-active" : ""}`}
            size="xs"
            variant="ghost"
            onClick={() => setTool(tool === "razor" ? "select" : "razor")}
          >
            <Scissors data-icon="inline-start" />
            Razor <kbd>C</kbd>
          </Button>
          <IconButton label="Split selected clip at playhead (Enter)" disabled={!selected} onClick={() => {
            const clip = selected ? project.clips[selected] : undefined;
            if (clip && playhead > clip.position && playhead < clip.position + clip.out_point - clip.in_point) void onSplit(clip.id, playhead);
          }}><Scissors /></IconButton>
          <IconButton label="Remove selected clip (Delete)" disabled={!selected} onClick={() => void onRemove()}><Trash2 /></IconButton>
          <Separator orientation="vertical" className="mx-1 h-4" />
          <Button variant="ghost" size="xs" onClick={onPreviewTimeline}>
            <Play data-icon="inline-start" />
            Preview
          </Button>
          <Button
            variant="ghost"
            size="xs"
            onClick={() => void onAddTrack("Video")}
          >
            <Plus data-icon="inline-start" />
            Video
          </Button>
          <Button
            variant="ghost"
            size="xs"
            onClick={() => void onAddTrack("Audio")}
          >
            <Plus data-icon="inline-start" />
            Audio
          </Button>
        </div>
      </div>
      <div className="grid h-10 grid-cols-[148px_minmax(0,1fr)] border-b">
        <div className="border-r" />
        <div
          className="timeline-ruler relative"
          onPointerDown={(event) => setPlayhead(timeAtPointer(event, event.currentTarget))}
          onPointerMove={(event) => tool === "razor" && setRazorPosition(timeAtPointer(event, event.currentTarget))}
        >
          {rulerTicks.map((tick) => (
            <span
              key={tick}
              className="absolute top-3 font-mono text-[10px] text-muted-foreground"
              style={{ left: `${(tick / duration) * 100}%` }}
            >{timecode(tick)}</span>
          ))}
        </div>
      </div>
      <div className="relative overflow-auto" style={{ height: "calc(100% - 88px)" }}>
        {project.tracks.length === 0 && (
          <div className="grid h-full place-items-center text-center">
            <div>
              <p className="text-sm font-medium">Your timeline is empty</p>
              <p className="mt-1 text-xs text-muted-foreground">
                Import a take or add a track to begin.
              </p>
            </div>
          </div>
        )}
        {project.tracks.map((track) => (
          <div
            key={track.id}
            data-track-id={track.id}
            className="grid h-16 grid-cols-[148px_1fr] border-b"
          >
            <div className="border-r px-4 py-3">
              <p className="font-mono text-xs">{track.name}</p>
              <p className="mt-1 text-[10px] uppercase tracking-wider text-muted-foreground">
                {track.kind}
              </p>
            </div>
            <div
              className="timeline-lane relative"
              data-timeline-lane
              onPointerDown={(event) => {
                if (event.target === event.currentTarget) setPlayhead(timeAtPointer(event, event.currentTarget));
              }}
              onPointerMove={(event) => tool === "razor" && setRazorPosition(timeAtPointer(event, event.currentTarget))}
            >
              {(draggingClip?.trackId === track.id && !track.clip_order.includes(draggingClip.id)
                ? [...track.clip_order, draggingClip.id]
                : track.clip_order
              ).map((id) => {
                const clip = project.clips[id];
                if (!clip) return null;
                if (draggingClip?.id === id && draggingClip.trackId !== track.id) return null;
                const displayPosition = draggingClip?.id === id ? draggingClip.position : clip.position;
                const left = (displayPosition / duration) * 100;
                const width = ((clip.out_point - clip.in_point) / duration) * 100;
                return (
                  <button
                    key={id}
                    onPointerDown={(event) => {
                      if (tool === "razor") {
                        event.stopPropagation();
                        const lane = event.currentTarget.closest<HTMLElement>("[data-timeline-lane]");
                        if (lane) splitClipAt(clip, timeAtPointer(event, lane));
                        return;
                      }
                      moveClip(event, clip);
                    }}
                    className={`timeline-clip ${selected === id ? "is-selected" : ""} ${draggingClip?.id === id ? "is-moving" : ""} ${tool === "razor" ? "is-razor" : ""}`}
                    style={{ left: `${left}%`, width: `${width}%` }}
                  >
                    {selected === id && tool === "select" && <>
                      <span className="timeline-trim-handle timeline-trim-start" role="button" aria-label="Trim clip start" onPointerDown={(event) => trimClip(event, clip, "start")} />
                      <span className="timeline-trim-handle timeline-trim-end" role="button" aria-label="Trim clip end" onPointerDown={(event) => trimClip(event, clip, "end")} />
                    </>}
                    <Clapperboard />
                    <span>{fileName(clip.source_path)}</span>
                    {clip.stabilization && (
                      <Badge variant="outline">STAB</Badge>
                    )}
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </div>
      <div className="pointer-events-none absolute bottom-0 left-[148px] right-0 top-[52px] z-10">
        {tool === "razor" && razorPosition !== undefined && (
          <div
            className="timeline-razor-marker absolute bottom-0 top-0"
            style={{ left: `${(razorPosition / duration) * 100}%` }}
          />
        )}
        <div
          className="timeline-playhead-line absolute bottom-0 top-0"
          style={{ left: `${(playhead / duration) * 100}%` }}
        />
        <button
          aria-label="Drag playhead"
          className="timeline-playhead pointer-events-auto absolute top-0"
          onPointerDown={movePlayhead}
          style={{ left: `${(playhead / duration) * 100}%` }}
        />
      </div>
    </section>
  );
}
function SettingsDialog({
  open,
  setOpen,
  provider,
  project,
  notice,
  setProvider,
  saveProvider,
  appVersion,
  autoCheckUpdates,
  setAutoCheckUpdates,
  timelineGrid,
  setTimelineGrid,
  updateCheck,
  checkingUpdate,
  downloadingUpdate,
  checkForUpdates,
  downloadUpdate,
  mediaDiagnostics,
  checkingMediaDiagnostics,
  checkMediaDiagnostics,
}: {
  open: boolean;
  setOpen: (value: boolean) => void;
  provider: Provider;
  project: Project;
  notice: string;
  setProvider: (value: Provider) => void;
  saveProvider: (test?: boolean) => Promise<void>;
  appVersion: string;
  autoCheckUpdates: boolean;
  setAutoCheckUpdates: (value: boolean) => void;
  timelineGrid: TimelineGrid;
  setTimelineGrid: (value: TimelineGrid) => void;
  updateCheck?: UpdateCheckResult;
  checkingUpdate: boolean;
  downloadingUpdate: boolean;
  checkForUpdates: () => Promise<void>;
  downloadUpdate: () => Promise<void>;
  mediaDiagnostics?: MediaDiagnostics;
  checkingMediaDiagnostics: boolean;
  checkMediaDiagnostics: () => Promise<void>;
}) {
  const set = (key: keyof Provider, value: string | null) =>
    setProvider({ ...provider, [key]: value });
  useEffect(() => {
    if (open && !mediaDiagnostics && !checkingMediaDiagnostics) void checkMediaDiagnostics();
  }, [open, mediaDiagnostics, checkingMediaDiagnostics, checkMediaDiagnostics]);
  const connectionNotice =
    notice === "Testing AI connection…" ||
    notice === "AI provider connected" ||
    notice.startsWith("AI connection failed:")
      ? notice
      : undefined;
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Editor preferences, connected services, and application details.
          </DialogDescription>
        </DialogHeader>
        <Tabs defaultValue="general">
          <TabsList className="w-full">
            <TabsTrigger value="general">General</TabsTrigger>
            <TabsTrigger value="ai">AI</TabsTrigger>
            <TabsTrigger value="media">Media tools</TabsTrigger>
            <TabsTrigger value="updates">Updates</TabsTrigger>
            <TabsTrigger value="info">Info</TabsTrigger>
          </TabsList>
          <TabsContent value="general" className="mt-4 space-y-3">
            <Card size="sm" className="shadow-none">
              <CardHeader className="pb-3">
                <CardTitle className="text-sm">Timeline grid</CardTitle>
                <CardDescription className="text-xs">Choose how much visual guidance appears behind clips and in the ruler.</CardDescription>
              </CardHeader>
              <CardContent className="pt-0">
                <ToggleGroup
                  type="single"
                  value={timelineGrid}
                  onValueChange={(value) => {
                    if (value) setTimelineGrid(value as TimelineGrid);
                  }}
                  variant="outline"
                  size="sm"
                  spacing={0}
                  aria-label="Timeline grid density"
                  className="w-full"
                >
                  <ToggleGroupItem value="off" className="flex-1 text-xs">Off</ToggleGroupItem>
                  <ToggleGroupItem value="coarse" className="flex-1 text-xs">Coarse</ToggleGroupItem>
                  <ToggleGroupItem value="medium" className="flex-1 text-xs">Medium</ToggleGroupItem>
                  <ToggleGroupItem value="fine" className="flex-1 text-xs">Fine</ToggleGroupItem>
                </ToggleGroup>
              </CardContent>
            </Card>
            <Card size="sm" className="shadow-none">
              <CardHeader className="pb-2">
                <CardTitle className="text-sm">Preview processing</CardTitle>
                <CardDescription className="text-xs">Interactive previews are rendered at up to 960px wide with one background worker.</CardDescription>
              </CardHeader>
            </Card>
            <Card size="sm" className="shadow-none">
              <CardHeader className="pb-2">
                <CardTitle className="text-sm">Current project</CardTitle>
                <CardDescription className="font-mono text-xs">{project.width} × {project.height} · {project.fps} FPS · {Object.keys(project.clips).length} clips</CardDescription>
              </CardHeader>
            </Card>
          </TabsContent>
          <TabsContent value="ai" className="mt-4 space-y-3">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="provider-base-url">Base URL</Label>
              <Input id="provider-base-url" value={provider.base_url} onChange={(event) => set("base_url", event.target.value)} placeholder="http://localhost:11434/v1" />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="provider-model">Model</Label>
              <Input id="provider-model" value={provider.model} onChange={(event) => set("model", event.target.value)} placeholder="llama3.2 or gpt-4o-mini" />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="provider-api-key">API key <span className="font-normal text-muted-foreground">(optional)</span></Label>
              <Input id="provider-api-key" type="password" value={provider.api_key ?? ""} onChange={(event) => set("api_key", event.target.value || null)} placeholder="sk-…" />
            </div>
            <Button variant="outline" className="w-full" onClick={() => void saveProvider(true)}>
              Test connection
            </Button>
            {connectionNotice && (
              <p
                role="status"
                className={`text-xs ${connectionNotice.startsWith("AI connection failed:") ? "text-destructive" : "text-muted-foreground"}`}
              >
                {connectionNotice}
              </p>
            )}
          </TabsContent>
          <TabsContent value="media" className="mt-4 space-y-3">
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-sm font-medium">FFmpeg diagnostics</p>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">The tools FPV Editor uses to inspect, preview, and export media.</p>
              </div>
              <Button variant="outline" size="sm" disabled={checkingMediaDiagnostics} onClick={() => void checkMediaDiagnostics()}>
                <RefreshCw data-icon="inline-start" className={checkingMediaDiagnostics ? "animate-spin" : ""} />
                {checkingMediaDiagnostics ? "Checking…" : "Refresh"}
              </Button>
            </div>
            {mediaDiagnostics ? <div className="space-y-2">
              {[mediaDiagnostics.ffmpeg, mediaDiagnostics.ffprobe].map((tool) => {
                const healthy = tool.status === "healthy";
                const missing = tool.status === "missing";
                return <Card key={tool.binary} size="sm" className="shadow-none">
                  <CardHeader className="pb-2">
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <CardTitle className="flex items-center gap-2 text-sm">
                          <Circle className={healthy ? "size-2 fill-emerald-500 text-emerald-500" : missing ? "size-2 fill-amber-500 text-amber-500" : "size-2 fill-destructive text-destructive"} />
                          {tool.binary}
                        </CardTitle>
                        <CardDescription className="mt-1 text-xs">{tool.message}</CardDescription>
                      </div>
                      <Badge variant={healthy ? "secondary" : "outline"} className={healthy ? "text-emerald-700 dark:text-emerald-400" : missing ? "border-amber-500/50 text-amber-700 dark:text-amber-400" : "border-destructive/50 text-destructive"}>{healthy ? "Ready" : missing ? "Missing" : "Needs attention"}</Badge>
                    </div>
                  </CardHeader>
                  <CardContent className="space-y-1.5 pt-0 text-[11px] leading-4 text-muted-foreground">
                    <p><span className="font-medium text-foreground">Selected from:</span> {tool.source}</p>
                    <p className="break-all font-mono">{tool.path}</p>
                    {tool.version && <p className="break-words font-mono">{tool.version}</p>}
                  </CardContent>
                </Card>;
              })}
            </div> : <Card size="sm" className="shadow-none"><CardContent className="flex items-center gap-2 py-4 text-xs text-muted-foreground"><RefreshCw className="size-3 animate-spin" /> Checking installed media tools…</CardContent></Card>}
          </TabsContent>
          <TabsContent value="updates" className="mt-4 space-y-3">
            <Card size="sm" className="shadow-none">
              <CardHeader className="pb-2">
                <div className="flex items-center justify-between">
                  <div>
                    <CardTitle className="text-sm">Software update</CardTitle>
                    <CardDescription className="font-mono text-xs">
                      Current version {appVersion ? `v${appVersion}` : "…"}
                    </CardDescription>
                  </div>
                  <Button variant="outline" size="sm" disabled={checkingUpdate} onClick={() => void checkForUpdates()}>
                    <RefreshCw data-icon="inline-start" className={checkingUpdate ? "animate-spin" : ""} />
                    {checkingUpdate ? "Checking…" : "Check for updates"}
                  </Button>
                </div>
              </CardHeader>
              <CardContent className="space-y-3 pt-0">
                {updateCheck && (
                  <div className="rounded-lg border p-3 text-xs">
                    {updateCheck.update_available ? (
                      <>
                        <div className="flex items-center justify-between">
                          <p className="font-medium">Update available: v{updateCheck.latest_version}</p>
                          <Badge variant="secondary">New</Badge>
                        </div>
                        {updateCheck.release_notes && (
                          <p className="mt-2 line-clamp-4 break-words whitespace-pre-line leading-5 text-muted-foreground">
                            {updateCheck.release_notes}
                          </p>
                        )}
                        <div className="mt-3 flex gap-2">
                          <Button
                            size="sm"
                            disabled={downloadingUpdate || !updateCheck.download_url}
                            onClick={() => void downloadUpdate()}
                          >
                            <Download data-icon="inline-start" />
                            {downloadingUpdate ? "Downloading…" : "Download & install"}
                          </Button>
                        </div>
                        {!updateCheck.download_url && (
                          <p className="mt-2 text-[10px] text-muted-foreground">
                            No installer for this platform was found in the release; open the release page instead.
                          </p>
                        )}
                      </>
                    ) : (
                      <p className="text-muted-foreground">You're up to date.</p>
                    )}
                  </div>
                )}
                <div className="flex items-center justify-between">
                  <div>
                    <p className="text-xs font-medium">Check automatically</p>
                    <p className="text-[10px] text-muted-foreground">Looks for a new release once when the app starts.</p>
                  </div>
                  <Switch checked={autoCheckUpdates} onCheckedChange={setAutoCheckUpdates} />
                </div>
              </CardContent>
            </Card>
          </TabsContent>
          <TabsContent value="info" className="mt-4">
            <Card size="sm" className="shadow-none">
              <CardHeader className="flex-row items-start gap-3 space-y-0">
                <div className="grid size-8 place-items-center rounded-md bg-secondary"><Info className="size-4" /></div>
                <div>
                  <CardTitle className="text-sm">FPV Editor</CardTitle>
                  <CardDescription className="mt-1 text-xs">Version {appVersion || "0.3.0"} · Desktop cut suite</CardDescription>
                </div>
              </CardHeader>
              <CardContent className="pt-0 text-xs leading-5 text-muted-foreground">Built with Tauri, Rust, React, and FFmpeg. Original media remains linked in place.</CardContent>
            </Card>
          </TabsContent>
        </Tabs>
        <DialogFooter>
          <Button
            onClick={() => {
              void saveProvider();
              setOpen(false);
            }}
          >
            Save settings
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
createRoot(document.getElementById("root")!).render(<App />);
