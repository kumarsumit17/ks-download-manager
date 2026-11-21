import type { LucideIcon } from "lucide-react";
import type { RefObject } from "react";
import { Clock, ListX, Pause, Play, Plus, Search, Settings, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

interface Action {
  label: string;
  icon: LucideIcon;
  enabled: boolean;
  primary?: boolean;
  onClick?: () => void;
  /** Shown as a tooltip; for a disabled action, why it is disabled. */
  hint?: string;
}

interface Props {
  canResume: boolean;
  canPause: boolean;
  /** Why Pause is disabled, when that is not obvious (a download that cannot resume). */
  pauseHint?: string;
  canDelete: boolean;
  canClearAll: boolean;
  onAdd: () => void;
  onResume: () => void;
  onPause: () => void;
  onDelete: () => void;
  onClearAll: () => void;
  query: string;
  onQuery: (q: string) => void;
  searchRef: RefObject<HTMLInputElement | null>;
}

const mod = navigator.userAgent.includes("Mac") ? "⌘" : "Ctrl+";

export function Toolbar(p: Props) {
  const primary: Action[] = [
    { label: "Add URL", icon: Plus, enabled: true, primary: true, onClick: p.onAdd, hint: `Add a download (${mod}N)` },
    { label: "Resume", icon: Play, enabled: p.canResume, onClick: p.onResume, hint: "Resume (Space)" },
    { label: "Pause", icon: Pause, enabled: p.canPause, onClick: p.onPause, hint: p.pauseHint ?? "Pause (Space)" },
    { label: "Delete", icon: Trash2, enabled: p.canDelete, onClick: p.onDelete, hint: "Delete (Delete key)" },
    { label: "Clear all", icon: ListX, enabled: p.canClearAll, onClick: p.onClearAll, hint: "Remove every download from the list" },
  ];
  // Queues and Settings arrive in Stage 5.
  const secondary: Action[] = [
    { label: "Queues", icon: Clock, enabled: false, hint: "Queues arrive in a later version" },
    { label: "Settings", icon: Settings, enabled: false, hint: "Settings arrive in a later version" },
  ];

  const render = (a: Action) => {
    const button = (
      <Button
        variant={a.primary ? "default" : "ghost"}
        className="h-14 w-[4.75rem] flex-col gap-1 text-xs"
        disabled={!a.enabled}
        onClick={a.onClick}
        aria-description={a.hint}
      >
        <a.icon className="size-5" strokeWidth={1.75} aria-hidden />
        {a.label}
      </Button>
    );
    if (!a.hint) return <span key={a.label}>{button}</span>;
    return (
      <Tooltip key={a.label}>
        {/* A disabled button gets no pointer events, so the tooltip hangs on a wrapper. */}
        <TooltipTrigger asChild>
          <span tabIndex={a.enabled ? undefined : 0} className="rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-ring">
            {button}
          </span>
        </TooltipTrigger>
        <TooltipContent className="max-w-xs">{a.hint}</TooltipContent>
      </Tooltip>
    );
  };

  return (
    <header role="toolbar" aria-label="Download actions" className="flex items-center gap-1 border-b bg-background px-2 py-1.5">
      {primary.map(render)}
      <Separator orientation="vertical" className="mx-1 !h-10" />
      {secondary.map(render)}
      <div className="ml-auto flex items-center gap-2 rounded-md border bg-muted/40 px-2 focus-within:ring-2 focus-within:ring-ring">
        <Search className="size-4 text-muted-foreground" strokeWidth={1.75} aria-hidden />
        <input
          ref={p.searchRef}
          type="search"
          value={p.query}
          onChange={(e) => p.onQuery(e.target.value)}
          placeholder="Search downloads"
          aria-label="Search downloads"
          className="h-8 w-52 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
        />
      </div>
    </header>
  );
}
