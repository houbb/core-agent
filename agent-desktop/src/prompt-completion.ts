import type { CommandSuggestion } from "./types";

export interface PromptCompletion {
  kind: "command" | "context";
  label: string;
  detail: string;
  replacement: string;
  start: number;
  end: number;
}

export interface MentionQuery {
  start: number;
  end: number;
  query: string;
}

export function mentionQueryAtCursor(input: string, cursor = input.length): MentionQuery | undefined {
  const left = input.slice(0, cursor);
  const start = left.lastIndexOf("@");
  if (start < 0 || (start > 0 && !/\s/.test(left[start - 1]))) return undefined;
  const raw = left.slice(start + 1);
  const quoted = raw.startsWith('"');
  const query = quoted ? raw.slice(1) : raw;
  if ((!quoted && /\s/.test(query)) || (quoted && query.includes('"'))) return undefined;
  return { start, end: cursor, query };
}

export function commandCompletions(input: string, commands: CommandSuggestion[]): PromptCompletion[] {
  if (!input.startsWith("/") || /\s/.test(input)) return [];
  const prefix = input.slice(1).toLocaleLowerCase();
  return commands
    .filter((command) => command.name.toLocaleLowerCase().startsWith(prefix))
    .slice(0, 20)
    .map((command) => ({
      kind: "command",
      label: command.usage,
      detail: command.summary,
      replacement: `/${command.name}${command.usage === `/${command.name}` ? "" : " "}`,
      start: 0,
      end: input.length,
    }));
}

export function contextCompletions(
  input: string,
  paths: string[],
  cursor = input.length,
): PromptCompletion[] {
  const mention = mentionQueryAtCursor(input, cursor);
  if (!mention) return [];
  return paths.map((path) => ({
    kind: "context",
    label: `@${path}`,
    detail: "Workspace context",
    replacement: path.includes(" ") ? `@"${path.replaceAll('"', '\\"')}" ` : `@${path} `,
    start: mention.start,
    end: mention.end,
  }));
}

export function applyPromptCompletion(input: string, completion: PromptCompletion): string {
  return `${input.slice(0, completion.start)}${completion.replacement}${input.slice(completion.end)}`;
}
