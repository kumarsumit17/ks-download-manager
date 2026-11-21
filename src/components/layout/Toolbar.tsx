import type { LucideIcon } from "lucide-react";
import { Clock, ListX, Pause, Play, Plus, Search, Settings, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";

interface Action {
  label: string;
  icon: LucideIcon;
  enabled: boolean;
  primary?: boolean;
  onClick?: () => void;
}

interface Props {
  canResume: boolean;
  canPause: boolean;
  canDelete: boolean;
  canClearAll: boolean;
  onClearAll: () => void;
  query: string;
  onQuery: (q: string) => void;
}

export function Toolbar({ canResume, canPause, canDelete, canClearAll, onClearAll, query, onQuery }: Props) {
  const primary: Action[] = [
    { label: "Add URL", icon: Plus, enabled: true, primary: true },
    { label: "Resume", icon: Play, enabled: canResume },
    { label: "Pause", icon: Pause, enabled: canPause },
    { label: "Delete", icon: Trash2, enabled: canDelete },
    { label: "Clear all", icon: ListX, enabled: canClearAll, onClick: onClearAll },
  ];
  const secondary: Action[] = [
    { label: "Queues", icon: Clock, enabled: true },
    { label: "Settings", icon: Settings, enabled: true },
  ];

  const render = (a: Action) => (
    <Button
      key={a.label}
      variant={a.primary ? "default" : "ghost"}
      className="h-14 w-[4.75rem] flex-col gap-1 text-xs"
      disabled={!a.enabled}
      onClick={a.onClick}
      title={a.label}
    >
      <a.icon className="size-5" strokeWidth={1.75} aria-hidden />
      {a.label}
    </Button>
  );

  return (
    <header role="toolbar" aria-label="Download actions" className="flex items-center gap-1 border-b bg-background px-2 py-1.5">
      {primary.map(render)}
      <Separator orientation="vertical" className="mx-1 !h-10" />
      {secondary.map(render)}
      <div className="ml-auto flex items-center gap-2 rounded-md border bg-muted/40 px-2 focus-within:ring-2 focus-within:ring-ring">
        <Search className="size-4 text-muted-foreground" strokeWidth={1.75} aria-hidden />
        <input
          type="search"
          value={query}
          onChange={(e) => onQuery(e.target.value)}
          placeholder="Search downloads"
          aria-label="Search downloads"
          className="h-8 w-52 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
        />
      </div>
    </header>
  );
}
