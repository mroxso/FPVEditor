import { useCallback, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  ChevronLeft,
  ChevronRight,
  Circle,
  Clapperboard,
  Command,
  Download,
  FolderInput,
  FolderOpen,
  Layers2,
  Pause,
  Play,
  Plus,
  RotateCw,
  Send,
  Settings,
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
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
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

function App() {
  const [project, setProject] = useState<Project>(seedProject);
  const [selected, setSelected] = useState<string>();
  const [playhead, setPlayhead] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [undoable, setUndoable] = useState(false);
  const [redoable, setRedoable] = useState(false);
  const [notice, setNotice] = useState("Ready to edit");
  const [dragActive, setDragActive] = useState(false);
  const [copilotOpen, setCopilotOpen] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [provider, setProvider] = useState(defaultProvider);
  const [prompt, setPrompt] = useState("");
  const [messages, setMessages] = useState([
    {
      role: "ai",
      text: "I can help make this flight cut tighter. Ask me to stabilize, trim, or assemble a sequence.",
    },
  ]);
  const selectedClip = selected ? project.clips[selected] : undefined;
  const duration = useMemo(
    () =>
      Math.max(
        30_000_000,
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
  };
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
  }, []);
  useEffect(() => {
    if (!playing) return;
    const timer = window.setInterval(
      () =>
        setPlayhead((current) => (current >= duration ? 0 : current + 100_000)),
      100,
    );
    return () => window.clearInterval(timer);
  }, [playing, duration]);
  const addTrack = async (kind: TrackKind) =>
    command({
      command: "add_track",
      kind,
      name: `${kind === "Video" ? "V" : "A"}${project.tracks.filter((track) => track.kind === kind).length + 1}`,
    });
  const importPaths = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    try {
      setNotice(`Reading ${paths.length === 1 ? fileName(paths[0]) : `${paths.length} sources`}…`);
      const outcome = await invoke<MediaImportOutcome>("import_media", { paths });
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
          if (event.payload.type === "enter") setDragActive(true);
          if (event.payload.type === "leave") setDragActive(false);
          if (event.payload.type === "drop") {
            setDragActive(false);
            void importPaths(event.payload.paths);
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
        setNotice("Project saved");
      }
    } catch (error) {
      setNotice(`Save failed: ${String(error)}`);
    }
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
        setSelected(undefined);
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
        <header className="flex h-14 items-center border-b px-5">
          <div className="flex w-72 items-center gap-3">
            <div className="grid size-7 place-items-center rounded-md bg-primary text-primary-foreground">
              <Video />
            </div>
            <div>
              <p className="font-heading text-sm font-semibold">FPV Editor</p>
              <p className="text-[10px] uppercase tracking-[.16em] text-muted-foreground">
                Desktop cut suite
              </p>
            </div>
          </div>
          <div className="flex flex-1 items-center justify-center gap-2">
            <Circle className="size-2 fill-foreground" />
            <span className="font-mono text-xs">{project.name}</span>
            <span className="text-xs text-muted-foreground">
              {project.width} × {project.height} · {project.fps} fps
            </span>
          </div>
          <div className="flex w-72 justify-end gap-1">
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
            <IconButton label="Settings" onClick={() => setSettingsOpen(true)}>
              <Settings />
            </IconButton>
          </div>
        </header>
        <nav className="flex h-11 items-center gap-2 border-b px-4">
          <Button size="sm" onClick={() => void importMedia()}>
            <Plus data-icon="inline-start" />
            Import media
          </Button>
          <Button variant="outline" size="sm" onClick={() => void importFolder()}>
            <FolderInput data-icon="inline-start" />
            Import folder
          </Button>
          <Separator orientation="vertical" className="mx-1 h-4" />
          <IconButton
            label="Undo"
            disabled={!undoable}
            onClick={() => invoke<Outcome>("undo").then(apply)}
          >
            <Undo2 />
          </IconButton>
          <IconButton
            label="Redo"
            disabled={!redoable}
            onClick={() => invoke<Outcome>("redo").then(apply)}
          >
            <RotateCw />
          </IconButton>
          <p className="ml-2 flex-1 truncate text-xs text-muted-foreground">
            {notice}
          </p>
          <Button
            variant={copilotOpen ? "secondary" : "ghost"}
            size="sm"
            onClick={() => setCopilotOpen((open) => !open)}
          >
            <Sparkles data-icon="inline-start" />
            Copilot
          </Button>
        </nav>
        <section
          className={`grid min-h-0 ${copilotOpen ? "grid-cols-[230px_minmax(420px,1fr)_270px_310px]" : "grid-cols-[230px_minmax(420px,1fr)_270px]"}`}
          style={{ height: "calc(100vh - 315px)" }}
        >
          <aside className="border-r bg-card">
            <PanelTitle icon={<Layers2 />} title="Media" />
            <div className="p-3">
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
            </div>
          </aside>
          <section className="grid min-w-0 grid-rows-[minmax(0,1fr)_56px] bg-muted/30">
            <Preview
              selectedClip={selectedClip}
              playhead={playhead}
              duration={duration}
              playing={playing}
              setPlayhead={setPlayhead}
              setPlaying={setPlaying}
            />
          </section>
          <aside className="border-l bg-card">
            <PanelTitle icon={<Command />} title="Inspector" />
            {selectedClip ? (
              <ClipInspector clip={selectedClip} command={command} />
            ) : (
              <EmptyInspector />
            )}
          </aside>
          {copilotOpen && (
            <Copilot
              messages={messages}
              prompt={prompt}
              setPrompt={setPrompt}
              chat={chat}
              close={() => setCopilotOpen(false)}
            />
          )}
        </section>
        <Timeline
          project={project}
          selected={selected}
          duration={duration}
          playhead={playhead}
          onSelect={setSelected}
          onAddTrack={addTrack}
        />
        <SettingsDialog
          open={settingsOpen}
          setOpen={setSettingsOpen}
          provider={provider}
          setProvider={setProvider}
          saveProvider={saveProvider}
        />
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
function PanelTitle({ icon, title }: { icon: React.ReactNode; title: string }) {
  return (
    <div className="flex h-11 items-center gap-2 border-b px-3 text-xs font-medium">
      <span className="text-muted-foreground">{icon}</span>
      {title}
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
function Preview({
  selectedClip,
  playhead,
  duration,
  playing,
  setPlayhead,
  setPlaying,
}: {
  selectedClip?: Clip;
  playhead: number;
  duration: number;
  playing: boolean;
  setPlayhead: (value: number) => void;
  setPlaying: (value: boolean) => void;
}) {
  const source = selectedClip ? mediaSource(selectedClip.source_path) : undefined;
  return (
    <section className="grid min-w-0 grid-rows-[minmax(0,1fr)_56px]">
      <div className="grid place-items-center p-6">
        <div className="relative aspect-video w-[min(76vh,76%)] overflow-hidden rounded-lg border bg-card shadow-2xl">
          <div className="absolute inset-0 preview-grid" />
          <div className="absolute inset-[8%] border border-dashed border-border/70" />
          <div className="absolute left-[-10%] top-1/2 w-[120%] border-t border-foreground/30 -rotate-6" />
          {source ? (
            <video
              key={selectedClip?.id}
              className="absolute inset-0 size-full bg-black object-contain"
              controls
              preload="metadata"
              src={source}
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
          <div className="absolute bottom-3 left-4 right-4 flex justify-between font-mono text-[10px] text-muted-foreground">
            <span>{timecode(playhead)}</span>
            <span>60 FPS</span>
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
            onClick={() => setPlayhead(Math.max(0, playhead - 1_000_000))}
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
            onClick={() =>
              setPlayhead(Math.min(duration, playhead + 1_000_000))
            }
          >
            <ChevronRight />
          </IconButton>
        </div>
        <span className="font-mono text-xs text-muted-foreground">
          {timecode(duration)}
        </span>
      </div>
    </section>
  );
}
function ClipInspector({
  clip,
  command,
}: {
  clip: Clip;
  command: (value: object) => Promise<Outcome | undefined>;
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
        <div className="flex flex-col gap-3">
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
        </div>
        <Separator />
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
  onSelect,
  onAddTrack,
}: {
  project: Project;
  selected?: string;
  duration: number;
  playhead: number;
  onSelect: (id: string) => void;
  onAddTrack: (kind: TrackKind) => Promise<Outcome | undefined>;
}) {
  return (
    <section className="relative h-[220px] overflow-hidden border-t bg-card">
      <div className="grid h-10 grid-cols-[148px_1fr_188px] border-b">
        <div className="flex items-center gap-2 border-r px-4">
          <span className="text-xs font-medium">Timeline</span>
          <Badge variant="outline" className="font-mono text-[9px]">
            MAGNETIC
          </Badge>
        </div>
        <div className="timeline-ruler relative">
          {[0, 5, 10, 15, 20, 25, 30].map((second) => (
            <span
              key={second}
              className="absolute top-3 font-mono text-[10px] text-muted-foreground"
              style={{ left: `${(second / 30) * 100}%` }}
            >{`00:${String(second).padStart(2, "0")}`}</span>
          ))}
        </div>
        <div className="flex items-center justify-end gap-1 px-2">
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
      <div className="relative h-[180px] overflow-auto">
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
            className="grid h-16 grid-cols-[148px_1fr] border-b"
          >
            <div className="border-r px-4 py-3">
              <p className="font-mono text-xs">{track.name}</p>
              <p className="mt-1 text-[10px] uppercase tracking-wider text-muted-foreground">
                {track.kind}
              </p>
            </div>
            <div className="timeline-lane relative">
              {track.clip_order.map((id) => {
                const clip = project.clips[id];
                if (!clip) return null;
                const left = (clip.position / duration) * 100;
                const width = Math.max(
                  8,
                  ((clip.out_point - clip.in_point) / duration) * 100,
                );
                return (
                  <button
                    key={id}
                    onClick={() => onSelect(id)}
                    className={`timeline-clip ${selected === id ? "is-selected" : ""}`}
                    style={{ left: `${left}%`, width: `${width}%` }}
                  >
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
      <div
        className="absolute bottom-0 top-10 w-px bg-foreground"
        style={{
          left: `calc(148px + ${(playhead / duration) * 100}% * (100% - 148px) / 100)`,
        }}
      />
    </section>
  );
}
function SettingsDialog({
  open,
  setOpen,
  provider,
  setProvider,
  saveProvider,
}: {
  open: boolean;
  setOpen: (value: boolean) => void;
  provider: Provider;
  setProvider: (value: Provider) => void;
  saveProvider: (test?: boolean) => Promise<void>;
}) {
  const set = (key: keyof Provider, value: string | null) =>
    setProvider({ ...provider, [key]: value });
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>AI settings</DialogTitle>
          <DialogDescription>
            Connect FPV Editor to any OpenAI-compatible endpoint. Keys stay in
            this local app session.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="provider-base-url">Base URL</Label>
            <Input
              id="provider-base-url"
              value={provider.base_url}
              onChange={(event) => set("base_url", event.target.value)}
              placeholder="http://localhost:11434/v1"
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="provider-model">Model</Label>
            <Input
              id="provider-model"
              value={provider.model}
              onChange={(event) => set("model", event.target.value)}
              placeholder="llama3.2 or gpt-4o-mini"
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="provider-api-key">
              API key{" "}
              <span className="font-normal text-muted-foreground">
                (optional)
              </span>
            </Label>
            <Input
              id="provider-api-key"
              type="password"
              value={provider.api_key ?? ""}
              onChange={(event) => set("api_key", event.target.value || null)}
              placeholder="sk-…"
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => void saveProvider(true)}>
            Test connection
          </Button>
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
