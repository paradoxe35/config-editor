// Languages written in JavaScript for the custom-language specs, as source
// text a page evaluates (`registerLanguage(${HJSON})`):
//
// - ENV: `.env` files with only `parse`, `serialize`, `keySpan`,
//   `valueSpan`, `comments` and `separators`: schema completion and hover
//   come from the flat default of its separators.
// - HJSON: a small HJSON-like language (`key: value`, `key: {` … `}` on
//   their own lines, `#` comments, `'''` strings and `/* */` comments over
//   lines) with the `contextAt`, `insertKey`, `keyAt` and `multilineRanges`
//   callbacks.

export const ENV = `{
  label: "ENV",
  extensions: ["env"],
  comments: { lineComment: "#" },
  separators: ["="],
  parse(text) {
    const value = {};
    text.split("\\n").forEach((line, index) => {
      if (!line.trim() || line.trim().startsWith("#")) return;
      const at = line.indexOf("=");
      if (at < 0) throw { message: "expected KEY=value", line: index + 1, column: 1 };
      value[line.slice(0, at).trim()] = line.slice(at + 1).trim();
    });
    return value;
  },
  serialize(value) {
    return Object.entries(value).map(([key, item]) => key + "=" + item).join("\\n") + "\\n";
  },
  keySpan(text, pointer) {
    const lines = text.split("\\n");
    const index = lines.findIndex((line) => !line.trim().startsWith("#") && line.split("=")[0].trim() === pointer.slice(1));
    if (index < 0) return null;
    const start = lines[index].indexOf(pointer.slice(1));
    return { line: index + 1, startColumn: start + 1, endColumn: start + pointer.length };
  },
  valueSpan(text, pointer) {
    const lines = text.split("\\n");
    const index = lines.findIndex((line) => !line.trim().startsWith("#") && line.split("=")[0].trim() === pointer.slice(1));
    if (index < 0) return null;
    const line = lines[index];
    const start = line.indexOf("=") + 1 + (line.slice(line.indexOf("=") + 1).length - line.slice(line.indexOf("=") + 1).trimStart().length);
    return { line: index + 1, startColumn: start + 1, endColumn: line.trimEnd().length + 1 };
  },
}`;

export const HJSON = `(() => {
  // Each line with the keys of the objects open at its start (path), and
  // where its key and value are.
  function scan(text) {
    const out = [];
    const stack = [];
    let string = null;
    let comment = false;
    text.split("\\n").forEach((line, index) => {
      const info = { index, path: [...stack], kind: "blank" };
      out.push(info);
      const trimmed = line.trim();
      if (string) {
        info.kind = "inside";
        const end = line.indexOf("'''");
        if (end < 0) string.lines.push(line);
        else {
          string.entry.value = string.lines.join("\\n");
          string = null;
        }
        return;
      }
      if (comment) {
        info.kind = "inside";
        if (line.includes("*/")) comment = false;
        return;
      }
      if (trimmed.startsWith("/*")) {
        info.kind = "comment";
        comment = !trimmed.includes("*/", 2);
        return;
      }
      if (!trimmed || trimmed.startsWith("#")) {
        info.kind = trimmed ? "comment" : "blank";
        return;
      }
      if (trimmed === "}") {
        stack.pop();
        info.kind = "close";
        return;
      }
      const keyStart = line.length - line.trimStart().length;
      const colon = line.indexOf(":");
      if (colon < 0) {
        Object.assign(info, { kind: "partial", keyStart });
        return;
      }
      let rest = line.slice(colon + 1);
      const hash = rest.search(/\\s#/);
      if (hash >= 0) rest = rest.slice(0, hash);
      const valueStart = colon + 1 + rest.length - rest.trimStart().length;
      const raw = rest.trim();
      const key = line.slice(keyStart, colon).trim();
      Object.assign(info, { kind: "entry", key, keyStart, valueStart, valueEnd: valueStart + raw.length, raw });
      if (raw === "{") {
        info.kind = "open";
        stack.push(key);
      } else if (raw === "'''") {
        info.multiline = true;
        string = { entry: info, lines: [] };
      }
    });
    return out;
  }
  function scalar(raw) {
    if (raw === "true" || raw === "false") return raw === "true";
    if (raw === "null") return null;
    if (/^-?\\d+(\\.\\d+)?$/.test(raw)) return Number(raw);
    if (raw.startsWith('"') || raw.startsWith("[")) return JSON.parse(raw);
    return raw;
  }
  function write(value, indent) {
    return Object.entries(value).map(([key, item]) => {
      if (item !== null && typeof item === "object" && !Array.isArray(item)) {
        return indent + key + ": {\\n" + write(item, indent + "  ") + indent + "}\\n";
      }
      const bare = typeof item === "string" && /^[A-Za-z_][\\w.\\/-]*$/.test(item) && !["true", "false", "null"].includes(item);
      const text = bare ? item : JSON.stringify(item);
      return indent + key + ": " + text + "\\n";
    }).join("");
  }
  function find(text, pointer) {
    const want = pointer.split("/").slice(1).map((part) => part.replace(/~1/g, "/").replace(/~0/g, "~"));
    return scan(text).find((info) => (info.kind === "entry" || info.kind === "open")
      && info.path.length + 1 === want.length && info.path.every((key, at) => key === want[at]) && info.key === want.at(-1));
  }
  const lineAt = (text, offset) => text.slice(0, offset).split("\\n").length - 1;
  return {
    label: "HJSON",
    extensions: ["hjson"],
    comments: { lineComment: "#", blockComment: ["/*", "*/"] },
    brackets: [["{", "}"]],
    separators: [":"],
    parse(text) {
      const root = {};
      const objects = [root];
      for (const info of scan(text)) {
        const target = objects[objects.length - 1];
        if (info.kind === "partial") throw { message: "expected key: value", line: info.index + 1, column: info.keyStart + 1 };
        if (info.kind === "close") {
          if (objects.length === 1) throw { message: "unexpected }", line: info.index + 1, column: 1 };
          objects.pop();
        }
        if (info.kind === "open") objects.push((target[info.key] = {}));
        if (info.kind === "entry") target[info.key] = info.multiline ? info.value ?? "" : scalar(info.raw);
      }
      if (objects.length > 1) throw { message: "unclosed {" };
      return root;
    },
    serialize(value) {
      return write(value, "");
    },
    keySpan(text, pointer) {
      const info = find(text, pointer);
      return info && { line: info.index + 1, startColumn: info.keyStart + 1, endColumn: info.keyStart + info.key.length + 1 };
    },
    valueSpan(text, pointer) {
      const info = find(text, pointer);
      return info && info.kind === "entry" && !info.multiline
        ? { line: info.index + 1, startColumn: info.valueStart + 1, endColumn: info.valueEnd + 1 }
        : null;
    },
    highlight(line) {
      const trimmed = line.trimStart();
      const start = line.length - trimmed.length;
      if (trimmed.startsWith("#") || trimmed.startsWith("/*")) return [{ start, end: line.length, kind: "comment" }];
      const colon = line.indexOf(":");
      if (colon < 0) return [];
      const hash = line.search(/\\s#/);
      const end = hash > colon ? hash : line.length;
      const tokens = [
        { start, end: colon, kind: "key" },
        { start: colon, end: colon + 1, kind: "punctuation" },
        { start: colon + 1, end, kind: "value" },
      ];
      if (hash > colon) tokens.push({ start: hash + 1, end: line.length, kind: "comment" });
      return tokens;
    },
    contextAt(text, offset) {
      const info = scan(text)[lineAt(text, offset)];
      if (info.kind === "inside" || info.kind === "comment") return null;
      const before = text.slice(text.lastIndexOf("\\n", offset - 1) + 1, offset);
      if (before.includes("#")) return null;
      const colon = before.indexOf(":");
      if (colon < 0) return { path: info.path, at: "key", prefix: before.trim() };
      return { path: [...info.path, before.slice(0, colon).trim()], at: "value", prefix: before.slice(colon + 1).trimStart() };
    },
    insertKey(key, kind) {
      if (kind === "object") return { text: key + ": {\\n  \\n}", caret: key.length + 6 };
      if (kind === "array") return { text: key + ": []", caret: key.length + 3 };
      return { text: key + ": ", caret: key.length + 2 };
    },
    keyAt(text, offset) {
      const info = scan(text)[lineAt(text, offset)];
      if (info.kind !== "entry" && info.kind !== "open") return null;
      const lineStart = text.lastIndexOf("\\n", offset - 1) + 1;
      const column = offset - lineStart;
      if (column < info.keyStart || column >= info.keyStart + info.key.length) return null;
      return { key: info.key, path: [...info.path, info.key], start: lineStart + info.keyStart };
    },
    multilineRanges(text) {
      return [...text.matchAll(/'''[\\s\\S]*?(?:'''|$)|\\/\\*[\\s\\S]*?(?:\\*\\/|$)/g)].map((match) => ({
        start: match.index,
        end: match.index + match[0].length,
        kind: match[0].startsWith("'") ? "string" : "comment",
      }));
    },
  };
})()`;

/** The schema both languages are edited with. */
export const SCHEMA = JSON.stringify({
  type: 'object',
  properties: {
    LOG_LEVEL: { enum: ['debug', 'info', 'warn'], description: 'How much the server logs' },
    DEBUG: { type: 'boolean', description: 'Turns on the debug endpoints' },
    PORT: { type: 'string', description: 'The port to listen on' },
    PATH_PREFIX: { type: 'string', description: 'Served under this path' },
    server: {
      type: 'object',
      description: 'The HTTP server',
      properties: {
        host: { type: 'string', description: 'Host name the server answers to' },
        mode: { enum: ['fast', 'safe'], description: 'How requests are handled' },
        tls: { type: 'object', description: 'TLS settings', properties: { enabled: { type: 'boolean' } } },
        hosts: { type: 'array', items: { type: 'string' } },
      },
    },
  },
});
