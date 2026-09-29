// Slash commands that narrow the search, shown as a pill in the search bar
// instead of as text: typing "/files " turns into a Files pill.

export interface Scope {
  /** The slash command, without the slash. */
  command: string;
  label: string;
  glyph: string;
  aliases: string[];
}

export const SCOPES: Scope[] = [
  { command: "apps", label: "Apps", glyph: "apps", aliases: ["app", "applications"] },
  { command: "files", label: "Files", glyph: "file", aliases: ["f", "file"] },
  { command: "clip", label: "Clipboard", glyph: "clipboard", aliases: ["clipboard", "c"] },
  { command: "snip", label: "Snippets", glyph: "snippet", aliases: ["snippet", "snippets"] },
  { command: "note", label: "Notes", glyph: "note", aliases: ["n", "notes"] },
  { command: "system", label: "System", glyph: "shutdown", aliases: ["sys"] },
  { command: "google", label: "Google", glyph: "web", aliases: ["g"] },
  { command: "youtube", label: "YouTube", glyph: "web", aliases: ["yt"] },
  { command: "wiki", label: "Wikipedia", glyph: "web", aliases: ["w", "wikipedia"] },
  { command: "calc", label: "Calculator", glyph: "calculator", aliases: ["="] },
  { command: "run", label: "Run", glyph: "run", aliases: [] },
];

/** The buttons in the search bar, in order: Ctrl+1 is the first. */
export const BUTTONS: (Scope | "commands")[] = [
  ...["apps", "files", "clip", "snip", "note"].map((c) => SCOPES.find((s) => s.command === c)!),
  "commands",
];

/** The scope a slash command (or one of its aliases) stands for. */
export function scopeNamed(name: string): Scope | undefined {
  const lower = name.toLowerCase();
  return SCOPES.find((s) => s.command === lower || s.aliases.includes(lower));
}

/** Splits typed text like "/files budget" into its scope and the rest,
 * once the command is followed by a space. */
export function splitScope(text: string): { scope: Scope; rest: string } | null {
  const match = /^\/(\S+)\s(.*)$/s.exec(text);
  const scope = match && scopeNamed(match[1]);
  return scope ? { scope, rest: match[2] } : null;
}
